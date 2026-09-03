//! mailstore — SQLite ストレージ層。
//!
//! 設計方針:
//! - スキーマは `schema.sql` に集約。`Store::open` が冪等に適用する。
//! - 生の .eml はファイル（`data/mail/<account>/<folder>/<uid>.eml`）に置き、
//!   DB にはパースした結果と `raw_path` だけを持つ。MCP が落ちていても
//!   Claude がファイルとして読める保険。
//! - 全文検索は FTS5 trigram。日本語の部分一致が効く。

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use mailcore::{Account, AccountKind, Address, MessageSummary};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA_VERSION: i64 = 1;
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
        self.conn
            .execute_batch(SCHEMA_SQL)
            .context("apply schema")?;
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match current.as_deref().and_then(|v| v.parse::<i64>().ok()) {
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) if v > SCHEMA_VERSION => {
                anyhow::bail!("database schema {v} is newer than this build ({SCHEMA_VERSION})")
            }
            _ => {
                // 将来: v -> SCHEMA_VERSION の段階的マイグレーションをここに追加
                self.conn.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
        }
        Ok(())
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
        let rows = stmt.query_map([], |r| {
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
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
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
}
