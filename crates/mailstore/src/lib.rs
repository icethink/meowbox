//! mailstore — SQLite ストレージ層。
//!
//! 設計方針:
//! - スキーマは `schema.sql` に集約。`Store::open` が冪等に適用する。
//! - 生の .eml はファイル（`<data_dir>/mail/<account_id>/<folder>/<uid>.eml`）に置き、
//!   DB にはパースした結果と `raw_path` だけを持つ。MCP が落ちていても
//!   Claude がファイルとして読める保険。`data_dir` は `paths` モジュールが決める。
//! - 全文検索は FTS5 trigram。日本語の部分一致が効く。

pub mod paths;

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use mailcore::{Account, AccountKind, Address, Message, MessageSummary, ThreadSummary};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA_VERSION: i64 = 2;
const SCHEMA_SQL: &str = include_str!("schema.sql");

pub struct Store {
    conn: Connection,
}

impl Store {
    /// DB を開き、スキーマを適用する。存在しなければ作る。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("open sqlite")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// テスト用インメモリ DB。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        // meta テーブルだけ先に作る。schema_version を読んでから SCHEMA_SQL を流さないと、
        // 既存 DB では CREATE TABLE IF NOT EXISTS が no-op になり ALTER 前の列のまま止まる。
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .context("create meta table")?;
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let parsed = current.as_deref().and_then(|v| v.parse::<i64>().ok());

        // schema_version の行が無い、もしくは数値としてパースできない場合でも、
        // messages テーブルが既にあれば実体のある DB とみなす。v1 が唯一の
        // 既存バージョンなので from = 1 とする。messages も無ければ本当に
        // 新規の DB で、この場合だけ upgrade を呼ばない。
        // 将来 v3 を足すときも、ここで from を決めてから upgrade(from) が
        // 段階的に走る形は変わらない。
        let from = match parsed {
            Some(v) => Some(v),
            None if self.table_exists("messages")? => Some(1),
            None => None,
        };

        self.conn
            .execute_batch(SCHEMA_SQL)
            .context("apply schema")?;

        if let Some(v) = from {
            if v > SCHEMA_VERSION {
                anyhow::bail!("database schema {v} is newer than this build ({SCHEMA_VERSION})");
            }
            if v < SCHEMA_VERSION {
                self.upgrade(v)?;
            }
        }

        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    /// `from` から `SCHEMA_VERSION` への段階的マイグレーション。
    /// 新しいバージョンを足すときは `if from < N { ... }` を積み増していく。
    fn upgrade(&self, from: i64) -> Result<()> {
        if from < 2 {
            // v2: アーカイブ状態を持つ is_archived 列を messages に追加
            let has_column = self
                .conn
                .prepare("PRAGMA table_info(messages)")?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == "is_archived");
            if !has_column {
                self.conn.execute_batch(
                    "ALTER TABLE messages ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0",
                )?;
            }
        }
        Ok(())
    }

    /// 指定した名前のテーブルが存在するか。
    fn table_exists(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![name],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ---- accounts -------------------------------------------------------

    pub fn add_account(
        &self,
        name: &str,
        kind: AccountKind,
        email: &str,
        project_tag: Option<&str>,
        settings: &serde_json::Value,
    ) -> Result<Account> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO accounts(name, kind, email, project_tag, settings_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                name,
                kind.as_str(),
                email,
                project_tag,
                settings.to_string(),
                now.to_rfc3339()
            ],
        )?;
        Ok(Account {
            id: self.conn.last_insert_rowid(),
            name: name.to_string(),
            kind,
            email: email.to_string(),
            project_tag: project_tag.map(str::to_string),
            settings: settings.clone(),
            created_at: now,
        })
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, email, project_tag, settings_json, created_at
             FROM accounts ORDER BY id",
        )?;
        let rows = stmt.query_map([], row_to_account)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// 1 アカウントを id で引く。見つからなければ `Ok(None)`。
    pub fn get_account(&self, id: i64) -> Result<Option<Account>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, kind, email, project_tag, settings_json, created_at
                 FROM accounts WHERE id = ?1",
                params![id],
                row_to_account,
            )
            .optional()?;
        Ok(row)
    }

    /// アカウントを消す。folders / messages / attachments は
    /// ON DELETE CASCADE で一緒に消える。消せたら true。
    pub fn delete_account(&self, id: i64) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    // ---- folders --------------------------------------------------------

    /// フォルダを取得または作成し、id を返す。
    pub fn ensure_folder(&self, account_id: i64, path: &str, role: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO folders(account_id, path, role) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id, path) DO UPDATE SET role = excluded.role",
            params![account_id, path, role],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND path = ?2",
            params![account_id, path],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn folder_last_uid(&self, folder_id: i64) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT last_uid FROM folders WHERE id = ?1",
            params![folder_id],
            |r| r.get::<_, i64>(0),
        )? as u32)
    }

    pub fn set_folder_last_uid(&self, folder_id: i64, uid: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE folders SET last_uid = MAX(last_uid, ?2) WHERE id = ?1",
            params![folder_id, uid as i64],
        )?;
        Ok(())
    }

    pub fn folder_uidvalidity(&self, folder_id: i64) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row(
                "SELECT uidvalidity FROM folders WHERE id = ?1",
                params![folder_id],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .map(|v| v as u32))
    }

    pub fn set_folder_uidvalidity(&self, folder_id: i64, uidvalidity: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE folders SET uidvalidity = ?2 WHERE id = ?1",
            params![folder_id, uidvalidity as i64],
        )?;
        Ok(())
    }

    /// UIDVALIDITY が変わったとき用。last_uid を 0 に戻す。
    pub fn reset_folder_uid(&self, folder_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE folders SET last_uid = 0 WHERE id = ?1",
            params![folder_id],
        )?;
        Ok(())
    }

    // ---- messages -------------------------------------------------------

    /// パース済みメッセージを 1 件保存。同じ (folder, uid) は無視する。
    #[allow(clippy::too_many_arguments)]
    pub fn insert_message(&self, m: &NewMessage<'_>) -> Result<Option<i64>> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO messages(
                account_id, folder_id, uid, message_id, thread_key,
                from_addr, from_name, to_json, cc_json, subject, date,
                snippet, body_text, body_html, has_attachments, is_read, is_flagged, raw_path
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![
                m.account_id,
                m.folder_id,
                m.uid as i64,
                m.message_id,
                m.thread_key,
                m.from.email,
                m.from.name,
                serde_json::to_string(m.to)?,
                serde_json::to_string(m.cc)?,
                m.subject,
                m.date.to_rfc3339(),
                m.snippet,
                m.body_text,
                m.body_html,
                m.has_attachments as i64,
                m.is_read as i64,
                m.is_flagged as i64,
                m.raw_path,
            ],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let id = self.conn.last_insert_rowid();
        self.set_folder_last_uid(m.folder_id, m.uid)?;
        Ok(Some(id))
    }

    /// 添付ファイルのメタデータだけを保存する。本体は raw .eml に残す方針のため
    /// `path` は常に NULL。
    pub fn insert_attachment_meta(
        &self,
        message_id: i64,
        filename: &str,
        mime: &str,
        size: usize,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO attachments(message_id, filename, mime, size, path)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![message_id, filename, mime, size as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// FTS5 全文検索。`query` が空なら新着順。
    pub fn search(&self, q: &SearchQuery<'_>) -> Result<Vec<MessageSummary>> {
        let mut sql = String::from(
            "SELECT m.id, m.account_id, m.thread_key, m.from_addr, m.from_name,
                    m.subject, m.date, m.snippet, m.is_read
             FROM messages m ",
        );
        let mut where_clauses: Vec<String> = Vec::new();
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(text) = q.text.filter(|t| !t.trim().is_empty()) {
            sql.push_str("JOIN messages_fts f ON f.rowid = m.id ");
            where_clauses.push("messages_fts MATCH ?".into());
            args.push(Box::new(fts_escape(text)));
        }
        if let Some(a) = q.account_id {
            where_clauses.push("m.account_id = ?".into());
            args.push(Box::new(a));
        }
        if let Some(tag) = q.project_tag {
            where_clauses
                .push("m.account_id IN (SELECT id FROM accounts WHERE project_tag = ?)".into());
            args.push(Box::new(tag.to_string()));
        }
        if let Some(since) = q.since {
            where_clauses.push("m.date >= ?".into());
            args.push(Box::new(since.to_rfc3339()));
        }
        if q.unread_only {
            where_clauses.push("m.is_read = 0".into());
        }
        if !where_clauses.is_empty() {
            sql.push_str("WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY m.date DESC LIMIT ?");
        args.push(Box::new(q.limit.max(1) as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            let date: String = r.get(6)?;
            Ok(MessageSummary {
                id: r.get(0)?,
                account_id: r.get(1)?,
                thread_key: r.get(2)?,
                from: Address {
                    email: r.get(3)?,
                    name: r.get(4)?,
                },
                subject: r.get(5)?,
                date: parse_ts(&date),
                snippet: r.get(7)?,
                is_read: r.get::<_, i64>(8)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// 1 通を本文つきで取得する。`folder_path` は folders を join して埋める。
    /// 見つからなければ `Ok(None)`。
    pub fn get_message(&self, id: i64) -> Result<Option<Message>> {
        let row = self
            .conn
            .query_row(
                "SELECT m.id, m.account_id, f.path, m.uid, m.message_id, m.thread_key,
                        m.from_addr, m.from_name, m.to_json, m.cc_json, m.subject, m.date,
                        m.snippet, m.body_text, m.has_attachments, m.is_read, m.is_flagged
                 FROM messages m
                 JOIN folders f ON f.id = m.folder_id
                 WHERE m.id = ?1",
                params![id],
                row_to_message,
            )
            .optional()?;
        Ok(row)
    }

    /// スレッド内の全メッセージを日時昇順で取得する（本文つき）。
    pub fn thread_messages(&self, thread_key: &str) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.account_id, f.path, m.uid, m.message_id, m.thread_key,
                    m.from_addr, m.from_name, m.to_json, m.cc_json, m.subject, m.date,
                    m.snippet, m.body_text, m.has_attachments, m.is_read, m.is_flagged
             FROM messages m
             JOIN folders f ON f.id = m.folder_id
             WHERE m.thread_key = ?1
             ORDER BY m.date ASC, m.id ASC",
        )?;
        let rows = stmt.query_map(params![thread_key], row_to_message)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// スレッド一覧。`q` の条件で絞り、最新メッセージの日時降順で返す。
    pub fn list_threads(&self, q: &ThreadQuery<'_>) -> Result<Vec<ThreadSummary>> {
        let mut m_where: Vec<String> = Vec::new();
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !q.include_archived {
            m_where.push("msg.is_archived = 0".into());
        }
        if let Some(a) = q.account_id {
            m_where.push("msg.account_id = ?".into());
            args.push(Box::new(a));
        }
        if let Some(tag) = q.project_tag {
            m_where.push("a.project_tag = ?".into());
            args.push(Box::new(tag.to_string()));
        }
        let m_where_sql = if m_where.is_empty() {
            String::new()
        } else {
            format!("AND {}", m_where.join(" AND "))
        };

        let mut thread_filters: Vec<&str> = Vec::new();
        if q.unread_only {
            thread_filters.push("g.unread_count > 0");
        }
        if q.flagged_only {
            thread_filters.push("g.is_flagged = 1");
        }
        let thread_filters_sql = if thread_filters.is_empty() {
            String::new()
        } else {
            format!("AND {}", thread_filters.join(" AND "))
        };

        let sql = format!(
            "WITH m AS (
               SELECT msg.* FROM messages msg
               JOIN accounts a ON a.id = msg.account_id
               WHERE 1=1 {m_where_sql}
             ),
             agg AS (
               SELECT thread_key,
                      COUNT(*) AS message_count,
                      SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END) AS unread_count,
                      MAX(has_attachments) AS has_attachments,
                      MAX(is_flagged) AS is_flagged
               FROM m GROUP BY thread_key
             ),
             latest AS (
               SELECT m.*, ROW_NUMBER() OVER (PARTITION BY thread_key ORDER BY date DESC, id DESC) AS rn
               FROM m
             )
             SELECT l.thread_key, l.id, l.account_id, acc.project_tag, l.subject,
                    l.from_addr, l.from_name, l.snippet, l.date,
                    g.message_count, g.unread_count, g.has_attachments, g.is_flagged
             FROM latest l
             JOIN agg g ON g.thread_key = l.thread_key
             JOIN accounts acc ON acc.id = l.account_id
             WHERE l.rn = 1 {thread_filters_sql}
             ORDER BY l.date DESC, l.id DESC
             LIMIT ? OFFSET ?"
        );

        let limit = if q.limit == 0 { 200 } else { q.limit } as i64;
        args.push(Box::new(limit));
        args.push(Box::new(q.offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            let date: String = r.get(8)?;
            Ok(ThreadSummary {
                thread_key: r.get(0)?,
                latest_message_id: r.get(1)?,
                account_id: r.get(2)?,
                project_tag: r.get(3)?,
                subject: r.get(4)?,
                from: Address {
                    email: r.get(5)?,
                    name: r.get(6)?,
                },
                snippet: r.get(7)?,
                last_date: parse_ts(&date),
                message_count: r.get(9)?,
                unread_count: r.get(10)?,
                has_attachments: r.get::<_, i64>(11)? != 0,
                is_flagged: r.get::<_, i64>(12)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// 既読/未読をまとめて更新する。`ids` が空なら何もせず 0 を返す。
    pub fn set_read(&self, ids: &[i64], read: bool) -> Result<usize> {
        self.set_flag_column("is_read", ids, read)
    }

    /// フラグをまとめて更新する。`ids` が空なら何もせず 0 を返す。
    pub fn set_flagged(&self, ids: &[i64], flagged: bool) -> Result<usize> {
        self.set_flag_column("is_flagged", ids, flagged)
    }

    /// アーカイブ状態をまとめて更新する。`ids` が空なら何もせず 0 を返す。
    pub fn set_archived(&self, ids: &[i64], archived: bool) -> Result<usize> {
        self.set_flag_column("is_archived", ids, archived)
    }

    /// `is_read` / `is_flagged` / `is_archived` のような 0/1 列を `ids` に対して一括更新する。
    /// `column` は呼び出し側が渡す固定リテラルのみを想定し、外部入力を SQL に直接
    /// 埋め込まない（インジェクションの余地を作らない）。
    fn set_flag_column(&self, column: &str, ids: &[i64], value: bool) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("UPDATE messages SET {column} = ?1 WHERE id IN ({placeholders})");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(ids.len() + 1);
        args.push(Box::new(value as i64));
        for id in ids {
            args.push(Box::new(*id));
        }
        let changed = self
            .conn
            .execute(&sql, rusqlite::params_from_iter(args.iter()))?;
        Ok(changed)
    }

    /// アカウントごとの未読数（アーカイブ除く）。
    pub fn unread_counts_by_account(&self) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT account_id, COUNT(*) FROM messages
             WHERE is_read = 0 AND is_archived = 0
             GROUP BY account_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// サイドバーの「ビュー」に出す件数。
    pub fn view_counts(&self) -> Result<ViewCounts> {
        let all: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT thread_key) FROM messages WHERE is_archived = 0",
            [],
            |r| r.get(0),
        )?;
        let unread: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT thread_key) FROM messages
             WHERE is_archived = 0 AND is_read = 0",
            [],
            |r| r.get(0),
        )?;
        let flagged: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT thread_key) FROM messages
             WHERE is_archived = 0 AND is_flagged = 1",
            [],
            |r| r.get(0),
        )?;
        let tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'open'",
            [],
            |r| r.get(0),
        )?;
        let drafts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM drafts WHERE status = 'draft'",
            [],
            |r| r.get(0),
        )?;
        Ok(ViewCounts {
            all,
            unread,
            flagged,
            tasks,
            drafts,
        })
    }
}

/// `list_accounts` / `get_account` で共有する行マッピング。
/// 列の並びは両方の SELECT で揃えてあること。
fn row_to_account(r: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let kind: String = r.get(2)?;
    let settings: String = r.get(5)?;
    let created: String = r.get(6)?;
    Ok(Account {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: AccountKind::parse(&kind).unwrap_or(AccountKind::Imap),
        email: r.get(3)?,
        project_tag: r.get(4)?,
        settings: serde_json::from_str(&settings).unwrap_or_default(),
        created_at: parse_ts(&created),
    })
}

/// `get_message` / `thread_messages` で共有する行マッピング。
/// 列の並びは両方の SELECT で揃えてあること。
fn row_to_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let to_json: String = r.get(8)?;
    let cc_json: String = r.get(9)?;
    let date: String = r.get(11)?;
    Ok(Message {
        id: r.get(0)?,
        account_id: r.get(1)?,
        folder_path: r.get(2)?,
        uid: r.get::<_, i64>(3)? as u32,
        message_id: r.get(4)?,
        thread_key: r.get(5)?,
        from: Address {
            email: r.get(6)?,
            name: r.get(7)?,
        },
        to: serde_json::from_str(&to_json).unwrap_or_default(),
        cc: serde_json::from_str(&cc_json).unwrap_or_default(),
        subject: r.get(10)?,
        date: parse_ts(&date),
        snippet: r.get(12)?,
        body_text: r.get(13)?,
        has_attachments: r.get::<_, i64>(14)? != 0,
        is_read: r.get::<_, i64>(15)? != 0,
        is_flagged: r.get::<_, i64>(16)? != 0,
    })
}

/// `insert_message` の入力。所有権を取らないので同期ループで使いやすい。
pub struct NewMessage<'a> {
    pub account_id: i64,
    pub folder_id: i64,
    pub uid: u32,
    pub message_id: Option<&'a str>,
    pub thread_key: &'a str,
    pub from: &'a Address,
    pub to: &'a [Address],
    pub cc: &'a [Address],
    pub subject: &'a str,
    pub date: DateTime<Utc>,
    pub snippet: &'a str,
    pub body_text: &'a str,
    pub body_html: Option<&'a str>,
    pub has_attachments: bool,
    pub is_read: bool,
    pub is_flagged: bool,
    pub raw_path: Option<&'a str>,
}

#[derive(Default)]
pub struct SearchQuery<'a> {
    pub text: Option<&'a str>,
    pub account_id: Option<i64>,
    pub project_tag: Option<&'a str>,
    pub since: Option<DateTime<Utc>>,
    pub unread_only: bool,
    pub limit: usize,
}

/// `list_threads` の絞り込み。
#[derive(Debug, Clone, Default)]
pub struct ThreadQuery<'a> {
    pub account_id: Option<i64>,
    pub project_tag: Option<&'a str>,
    /// 未読を含むスレッドだけ
    pub unread_only: bool,
    /// フラグ付きメッセージを含むスレッドだけ
    pub flagged_only: bool,
    /// アーカイブ済みメッセージも数える
    pub include_archived: bool,
    /// 0 のときは 200 とみなす
    pub limit: usize,
    pub offset: usize,
}

/// サイドバーの「ビュー」に出す件数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewCounts {
    pub all: i64,
    pub unread: i64,
    pub flagged: i64,
    pub tasks: i64,
    pub drafts: i64,
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// FTS5 の MATCH 構文に素の文字列を渡すと `-` や `:` で壊れるので、
/// 空白区切りの各語をダブルクォートで包む。
fn fts_escape(text: &str) -> String {
    text.split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(store: &Store) -> (i64, i64) {
        let acc = store
            .add_account(
                "test",
                AccountKind::Imap,
                "me@example.com",
                Some("案件A"),
                &serde_json::json!({}),
            )
            .unwrap();
        let folder = store.ensure_folder(acc.id, "INBOX", "inbox").unwrap();
        (acc.id, folder)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_msg(
        store: &Store,
        account_id: i64,
        folder_id: i64,
        uid: u32,
        thread_key: &str,
        subject: &str,
        from_name: Option<&str>,
        date: DateTime<Utc>,
        is_read: bool,
        is_flagged: bool,
        has_attachments: bool,
    ) -> i64 {
        let from = Address {
            name: from_name.map(str::to_string),
            email: "sender@example.com".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid,
            message_id: None,
            thread_key,
            from: &from,
            to: &[],
            cc: &[],
            subject,
            date,
            snippet: subject,
            body_text: subject,
            body_html: None,
            has_attachments,
            is_read,
            is_flagged,
            raw_path: None,
        };
        store.insert_message(&m).unwrap().unwrap()
    }

    #[test]
    fn schema_applies_and_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        store.migrate().unwrap();
        assert_eq!(store.list_accounts().unwrap().len(), 0);
    }

    #[test]
    fn insert_and_search_japanese() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let from = Address {
            name: Some("山田".into()),
            email: "yamada@client-a.example".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid: 1,
            message_id: Some("<a@b>"),
            thread_key: "見積の件",
            from: &from,
            to: &[],
            cc: &[],
            subject: "Re: 見積の件",
            date: Utc::now(),
            snippet: "お世話になっております",
            body_text: "お世話になっております。見積書を添付いたします。",
            body_html: None,
            has_attachments: true,
            is_read: false,
            is_flagged: false,
            raw_path: None,
        };
        assert!(store.insert_message(&m).unwrap().is_some());
        // 重複 UID は無視
        assert!(store.insert_message(&m).unwrap().is_none());
        assert_eq!(store.folder_last_uid(folder_id).unwrap(), 1);

        let hits = store
            .search(&SearchQuery {
                text: Some("見積書"),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].from.name.as_deref(), Some("山田"));

        let by_tag = store
            .search(&SearchQuery {
                project_tag: Some("案件A"),
                unread_only: true,
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_tag.len(), 1);

        let miss = store
            .search(&SearchQuery {
                text: Some("請求書"),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert!(miss.is_empty());
    }

    #[test]
    fn folder_uidvalidity_roundtrips() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let _ = account_id;
        assert_eq!(store.folder_uidvalidity(folder_id).unwrap(), None);
        store.set_folder_uidvalidity(folder_id, 12345).unwrap();
        assert_eq!(store.folder_uidvalidity(folder_id).unwrap(), Some(12345));
    }

    #[test]
    fn reset_folder_uid_zeroes_last_uid() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let from = Address {
            name: None,
            email: "sato@client-a.example".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid: 7,
            message_id: None,
            thread_key: "subj:x",
            from: &from,
            to: &[],
            cc: &[],
            subject: "x",
            date: Utc::now(),
            snippet: "x",
            body_text: "x",
            body_html: None,
            has_attachments: false,
            is_read: false,
            is_flagged: false,
            raw_path: None,
        };
        store.insert_message(&m).unwrap();
        assert_eq!(store.folder_last_uid(folder_id).unwrap(), 7);

        store.reset_folder_uid(folder_id).unwrap();
        assert_eq!(store.folder_last_uid(folder_id).unwrap(), 0);
    }

    #[test]
    fn get_message_roundtrips_and_returns_none_when_missing() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let from = Address {
            name: Some("山田".into()),
            email: "yamada@client-a.example".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid: 9,
            message_id: Some("<a@b>"),
            thread_key: "見積の件",
            from: &from,
            to: &[],
            cc: &[],
            subject: "Re: 見積の件",
            date: Utc::now(),
            snippet: "お世話になっております",
            body_text: "お世話になっております。見積書を添付いたします。",
            body_html: None,
            has_attachments: true,
            is_read: false,
            is_flagged: false,
            raw_path: None,
        };
        let id = store.insert_message(&m).unwrap().unwrap();

        let fetched = store.get_message(id).unwrap().unwrap();
        assert_eq!(fetched.subject, "Re: 見積の件");
        assert_eq!(
            fetched.body_text,
            "お世話になっております。見積書を添付いたします。"
        );
        assert_eq!(fetched.folder_path, "INBOX");
        assert_eq!(fetched.uid, 9);

        assert!(store.get_message(id + 1000).unwrap().is_none());
    }

    #[test]
    fn insert_attachment_meta_leaves_path_null() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let from = Address {
            name: None,
            email: "sato@client-a.example".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid: 1,
            message_id: None,
            thread_key: "subj:x",
            from: &from,
            to: &[],
            cc: &[],
            subject: "x",
            date: Utc::now(),
            snippet: "x",
            body_text: "x",
            body_html: None,
            has_attachments: true,
            is_read: false,
            is_flagged: false,
            raw_path: None,
        };
        let message_id = store.insert_message(&m).unwrap().unwrap();

        store
            .insert_attachment_meta(message_id, "notes.txt", "text/plain", 42)
            .unwrap();

        let (filename, mime, size, path): (String, String, i64, Option<String>) = store
            .conn()
            .query_row(
                "SELECT filename, mime, size, path FROM attachments WHERE message_id = ?1",
                params![message_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(filename, "notes.txt");
        assert_eq!(mime, "text/plain");
        assert_eq!(size, 42);
        assert_eq!(path, None);

        let count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE message_id = ?1",
                params![message_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn list_threads_folds_messages_by_thread_key() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let t0 = Utc::now() - chrono::Duration::hours(2);
        let t1 = Utc::now() - chrono::Duration::hours(1);
        let t2 = Utc::now();

        insert_msg(
            &store,
            account_id,
            folder_id,
            1,
            "t1",
            "見積の件",
            Some("山田"),
            t0,
            true,
            false,
            false,
        );
        insert_msg(
            &store,
            account_id,
            folder_id,
            2,
            "t1",
            "Re: 見積の件",
            Some("佐藤"),
            t1,
            false,
            false,
            false,
        );
        insert_msg(
            &store,
            account_id,
            folder_id,
            3,
            "t2",
            "別件",
            Some("鈴木"),
            t2,
            false,
            false,
            false,
        );

        let threads = store.list_threads(&ThreadQuery::default()).unwrap();
        assert_eq!(threads.len(), 2);

        let t1_summary = threads
            .iter()
            .find(|t| t.thread_key == "t1")
            .expect("t1 present");
        assert_eq!(t1_summary.subject, "Re: 見積の件");
        assert_eq!(t1_summary.from.name.as_deref(), Some("佐藤"));
        assert_eq!(t1_summary.message_count, 2);
        assert_eq!(t1_summary.unread_count, 1);

        let t2_summary = threads
            .iter()
            .find(|t| t.thread_key == "t2")
            .expect("t2 present");
        assert_eq!(t2_summary.message_count, 1);
        assert_eq!(t2_summary.unread_count, 1);
    }

    #[test]
    fn list_threads_excludes_archived() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let id = insert_msg(
            &store,
            account_id,
            folder_id,
            1,
            "t1",
            "件名",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        assert_eq!(
            store.list_threads(&ThreadQuery::default()).unwrap().len(),
            1
        );

        store.set_archived(&[id], true).unwrap();
        assert_eq!(
            store.list_threads(&ThreadQuery::default()).unwrap().len(),
            0
        );

        let with_archived = store
            .list_threads(&ThreadQuery {
                include_archived: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(with_archived.len(), 1);
    }

    #[test]
    fn list_threads_filters_by_project_tag() {
        let store = Store::open_in_memory().unwrap();
        let (account_a, folder_a) = seed(&store);
        let acc_b = store
            .add_account(
                "test-b",
                AccountKind::Imap,
                "b@example.com",
                Some("案件B"),
                &serde_json::json!({}),
            )
            .unwrap();
        let folder_b = store.ensure_folder(acc_b.id, "INBOX", "inbox").unwrap();

        insert_msg(
            &store,
            account_a,
            folder_a,
            1,
            "t1",
            "件名A",
            None,
            Utc::now(),
            false,
            false,
            false,
        );
        insert_msg(
            &store,
            acc_b.id,
            folder_b,
            1,
            "t2",
            "件名B",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        let by_a = store
            .list_threads(&ThreadQuery {
                project_tag: Some("案件A"),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_a.len(), 1);
        assert_eq!(by_a[0].thread_key, "t1");
    }

    #[test]
    fn list_threads_paginates() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let base = Utc::now();
        for i in 0..3 {
            insert_msg(
                &store,
                account_id,
                folder_id,
                i + 1,
                &format!("t{i}"),
                "件名",
                None,
                base - chrono::Duration::minutes(i as i64),
                false,
                false,
                false,
            );
        }

        let page = store
            .list_threads(&ThreadQuery {
                limit: 1,
                offset: 1,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].thread_key, "t1");
    }

    #[test]
    fn mark_updates_read_flag_and_archive() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let id = insert_msg(
            &store,
            account_id,
            folder_id,
            1,
            "t1",
            "件名",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        assert_eq!(store.set_read(&[id], true).unwrap(), 1);
        assert!(store.get_message(id).unwrap().unwrap().is_read);

        assert_eq!(store.set_flagged(&[id], true).unwrap(), 1);
        assert!(store.get_message(id).unwrap().unwrap().is_flagged);

        assert_eq!(store.set_archived(&[id], true).unwrap(), 1);

        assert_eq!(store.set_read(&[], true).unwrap(), 0);
        assert_eq!(store.set_flagged(&[], true).unwrap(), 0);
        assert_eq!(store.set_archived(&[], true).unwrap(), 0);
    }

    #[test]
    fn view_counts_counts_threads_not_messages() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        insert_msg(
            &store,
            account_id,
            folder_id,
            1,
            "t1",
            "件名1",
            None,
            Utc::now(),
            true,
            true,
            false,
        );
        insert_msg(
            &store,
            account_id,
            folder_id,
            2,
            "t1",
            "Re: 件名1",
            None,
            Utc::now(),
            false,
            false,
            false,
        );
        insert_msg(
            &store,
            account_id,
            folder_id,
            3,
            "t2",
            "件名2",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        store
            .conn()
            .execute(
                "INSERT INTO tasks(account_id, title, status, created_at)
                 VALUES (?1, 'task', 'open', ?2)",
                params![account_id, Utc::now().to_rfc3339()],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO drafts(account_id, status, created_at) VALUES (?1, 'draft', ?2)",
                params![account_id, Utc::now().to_rfc3339()],
            )
            .unwrap();

        let counts = store.view_counts().unwrap();
        assert_eq!(counts.all, 2);
        assert_eq!(counts.unread, 2);
        assert_eq!(counts.flagged, 1);
        assert_eq!(counts.tasks, 1);
        assert_eq!(counts.drafts, 1);
    }

    #[test]
    fn thread_messages_returns_the_thread_in_date_order() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        let t_early = Utc::now() - chrono::Duration::hours(2);
        let t_same = Utc::now() - chrono::Duration::hours(1);
        let t_late = Utc::now();

        // わざと日付昇順にならない順で挿入する。
        let id_late = insert_msg(
            &store, account_id, folder_id, 1, "t1", "遅い", None, t_late, false, false, false,
        );
        let id_early = insert_msg(
            &store, account_id, folder_id, 2, "t1", "早い", None, t_early, false, false, false,
        );
        let id_same_a = insert_msg(
            &store, account_id, folder_id, 3, "t1", "同着1", None, t_same, false, false, false,
        );
        let id_same_b = insert_msg(
            &store, account_id, folder_id, 4, "t1", "同着2", None, t_same, false, false, false,
        );
        insert_msg(
            &store,
            account_id,
            folder_id,
            5,
            "t2",
            "別スレッド",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        let messages = store.thread_messages("t1").unwrap();
        let ids: Vec<i64> = messages.iter().map(|m| m.id).collect();
        // 日付昇順、同日時なら id 昇順。
        assert_eq!(ids, vec![id_early, id_same_a, id_same_b, id_late]);
        assert!(messages.iter().all(|m| !m.body_text.is_empty()));
        assert!(messages.iter().all(|m| m.thread_key == "t1"));
    }

    #[test]
    fn unread_counts_by_account_counts_only_unread_and_unarchived() {
        let store = Store::open_in_memory().unwrap();
        let (account_a, folder_a) = seed(&store);
        let acc_b = store
            .add_account(
                "test-b",
                AccountKind::Imap,
                "b@example.com",
                Some("案件B"),
                &serde_json::json!({}),
            )
            .unwrap();
        let folder_b = store.ensure_folder(acc_b.id, "INBOX", "inbox").unwrap();

        // アカウント A: 未読1、既読1、未読だがアーカイブ済み1。
        insert_msg(
            &store,
            account_a,
            folder_a,
            1,
            "a1",
            "未読",
            None,
            Utc::now(),
            false,
            false,
            false,
        );
        insert_msg(
            &store,
            account_a,
            folder_a,
            2,
            "a2",
            "既読",
            None,
            Utc::now(),
            true,
            false,
            false,
        );
        let archived_id = insert_msg(
            &store,
            account_a,
            folder_a,
            3,
            "a3",
            "未読アーカイブ済み",
            None,
            Utc::now(),
            false,
            false,
            false,
        );
        store.set_archived(&[archived_id], true).unwrap();

        // アカウント B: 全て既読なので未読数 0。結果に出てこないはず。
        insert_msg(
            &store,
            acc_b.id,
            folder_b,
            1,
            "b1",
            "既読B",
            None,
            Utc::now(),
            true,
            false,
            false,
        );

        let counts = store.unread_counts_by_account().unwrap();
        assert_eq!(counts, vec![(account_a, 1)]);
    }

    #[test]
    fn migrates_v1_database_to_v2() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "meowbox_v1_migration_test_{}_{}.sqlite",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta(key, value) VALUES ('schema_version', '1');
                 CREATE TABLE accounts (
                     id            INTEGER PRIMARY KEY,
                     name          TEXT NOT NULL,
                     kind          TEXT NOT NULL,
                     email         TEXT NOT NULL UNIQUE,
                     project_tag   TEXT,
                     settings_json TEXT NOT NULL DEFAULT '{}',
                     created_at    TEXT NOT NULL
                 );
                 CREATE TABLE folders (
                     id           INTEGER PRIMARY KEY,
                     account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     path         TEXT NOT NULL,
                     role         TEXT NOT NULL DEFAULT 'other',
                     uidvalidity  INTEGER,
                     last_uid     INTEGER NOT NULL DEFAULT 0,
                     UNIQUE (account_id, path)
                 );
                 CREATE TABLE messages (
                     id              INTEGER PRIMARY KEY,
                     account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     folder_id       INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                     uid             INTEGER NOT NULL,
                     message_id      TEXT,
                     thread_key      TEXT NOT NULL,
                     from_addr       TEXT NOT NULL,
                     from_name       TEXT,
                     to_json         TEXT NOT NULL DEFAULT '[]',
                     cc_json         TEXT NOT NULL DEFAULT '[]',
                     subject         TEXT NOT NULL DEFAULT '',
                     date            TEXT NOT NULL,
                     snippet         TEXT NOT NULL DEFAULT '',
                     body_text       TEXT NOT NULL DEFAULT '',
                     body_html       TEXT,
                     has_attachments INTEGER NOT NULL DEFAULT 0,
                     is_read         INTEGER NOT NULL DEFAULT 0,
                     is_flagged      INTEGER NOT NULL DEFAULT 0,
                     raw_path        TEXT,
                     UNIQUE (folder_id, uid)
                 );
                 INSERT INTO accounts(id, name, kind, email, project_tag, settings_json, created_at)
                 VALUES (1, 'legacy', 'imap', 'legacy@example.com', NULL, '{}', '2024-01-01T00:00:00Z');
                 INSERT INTO folders(id, account_id, path, role) VALUES (1, 1, 'INBOX', 'inbox');
                 INSERT INTO messages(id, account_id, folder_id, uid, thread_key, from_addr, subject, date)
                 VALUES (1, 1, 1, 1, 'legacy-thread', 'a@example.com', 'legacy subject', '2024-01-01T00:00:00Z');",
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let has_column = store
            .conn()
            .prepare("PRAGMA table_info(messages)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|n| n == "is_archived");
        assert!(has_column);

        let threads = store.list_threads(&ThreadQuery::default()).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].thread_key, "legacy-thread");

        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn get_account_returns_none_for_a_missing_id() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, _folder_id) = seed(&store);
        assert!(store.get_account(account_id + 1000).unwrap().is_none());
    }

    #[test]
    fn delete_account_removes_the_account_and_its_messages() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, folder_id) = seed(&store);
        insert_msg(
            &store,
            account_id,
            folder_id,
            1,
            "t1",
            "件名",
            None,
            Utc::now(),
            false,
            false,
            false,
        );

        assert!(store.delete_account(account_id).unwrap());
        assert!(store.list_accounts().unwrap().is_empty());
        assert!(store
            .list_threads(&ThreadQuery::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn delete_account_returns_false_when_nothing_matched() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, _folder_id) = seed(&store);
        assert!(!store.delete_account(account_id + 1000).unwrap());
    }

    #[test]
    fn migrates_a_database_whose_version_row_is_missing() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "meowbox_missing_version_migration_test_{}_{}.sqlite",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let conn = Connection::open(&path).unwrap();
            // meta テーブル自体を作らない = schema_version の行が無い状態。
            conn.execute_batch(
                "CREATE TABLE accounts (
                     id            INTEGER PRIMARY KEY,
                     name          TEXT NOT NULL,
                     kind          TEXT NOT NULL,
                     email         TEXT NOT NULL UNIQUE,
                     project_tag   TEXT,
                     settings_json TEXT NOT NULL DEFAULT '{}',
                     created_at    TEXT NOT NULL
                 );
                 CREATE TABLE folders (
                     id           INTEGER PRIMARY KEY,
                     account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     path         TEXT NOT NULL,
                     role         TEXT NOT NULL DEFAULT 'other',
                     uidvalidity  INTEGER,
                     last_uid     INTEGER NOT NULL DEFAULT 0,
                     UNIQUE (account_id, path)
                 );
                 CREATE TABLE messages (
                     id              INTEGER PRIMARY KEY,
                     account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                     folder_id       INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                     uid             INTEGER NOT NULL,
                     message_id      TEXT,
                     thread_key      TEXT NOT NULL,
                     from_addr       TEXT NOT NULL,
                     from_name       TEXT,
                     to_json         TEXT NOT NULL DEFAULT '[]',
                     cc_json         TEXT NOT NULL DEFAULT '[]',
                     subject         TEXT NOT NULL DEFAULT '',
                     date            TEXT NOT NULL,
                     snippet         TEXT NOT NULL DEFAULT '',
                     body_text       TEXT NOT NULL DEFAULT '',
                     body_html       TEXT,
                     has_attachments INTEGER NOT NULL DEFAULT 0,
                     is_read         INTEGER NOT NULL DEFAULT 0,
                     is_flagged      INTEGER NOT NULL DEFAULT 0,
                     raw_path        TEXT,
                     UNIQUE (folder_id, uid)
                 );
                 INSERT INTO accounts(id, name, kind, email, project_tag, settings_json, created_at)
                 VALUES (1, 'legacy', 'imap', 'legacy-no-version@example.com', NULL, '{}', '2024-01-01T00:00:00Z');
                 INSERT INTO folders(id, account_id, path, role) VALUES (1, 1, 'INBOX', 'inbox');
                 INSERT INTO messages(id, account_id, folder_id, uid, thread_key, from_addr, subject, date)
                 VALUES (1, 1, 1, 1, 'legacy-thread-no-version', 'a@example.com', 'legacy subject', '2024-01-01T00:00:00Z');",
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();

        let has_column = store
            .conn()
            .prepare("PRAGMA table_info(messages)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|n| n == "is_archived");
        assert!(has_column);

        let version: String = store
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");

        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
