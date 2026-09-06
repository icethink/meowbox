//! 同期エンジン。アカウントごとに 1 タスク。
//!
//! `sync_once` の手順:
//! 1. `list_folders` → （`only_folder` があれば絞る）
//! 2. 各フォルダを `Store::ensure_folder`
//! 3. `folder_status` の UIDVALIDITY を保存済みと比較し、変わっていれば
//!    `reset_folder_uid` して取り直す
//! 4. `folder_last_uid` から `fetch_new` で差分を取得
//! 5. raw を `<data_dir>/<account_id>/<フォルダ名>/<uid>.eml` に保存し、
//!    `parse::parse` → `NewMessage` → `Store::insert_message`
//!
//! 1 通のパース失敗・1 フォルダの失敗で全体を止めない。`SyncReport` に数えて
//! 次へ進む。
//!
//! TODO(P4): INBOX は IMAP IDLE、他は 5〜15 分ポーリング。

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use mailcore::{Address, MailBackend, RawMessage};
use mailstore::{NewMessage, Store};

use crate::parse;

/// フォルダ内で処理した通数がこの数に達するごとに進捗を通知する。
const PROGRESS_EVERY: usize = 10;

pub struct SyncEngine {
    pub store: Arc<Store>,
}

/// 同期の途中経過。UI に逐次流すためのもの。
/// メール本文・アドレス・パスワードなど秘密情報は絶対に入れない（件数とフォルダ名だけ）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncProgress {
    /// いま処理しているフォルダ。`done = true` の最終通知では空文字列。
    pub folder: String,
    /// このフォルダで処理し終えた通数。
    pub fetched: usize,
    /// このフォルダでサーバから取得した総数。`fetched` の分母。
    pub total: usize,
    /// このフォルダで新規に DB へ入れた通数。
    pub inserted: usize,
    /// このフォルダで失敗した通数。
    pub errors: usize,
    /// アカウント 1 回分の同期が終わったときだけ true。
    /// このときの各件数はフォルダ単位ではなく `SyncReport` と同じ全体の合計。
    pub done: bool,
}

/// 進捗の通知先。`sync_once` の中から同期的に呼ばれるので、重い処理やブロックをしないこと。
pub type ProgressSink = std::sync::Arc<dyn Fn(SyncProgress) + Send + Sync>;

/// `sync_once` の挙動を調整するオプション。
pub struct SyncOptions {
    /// 指定があればこのフォルダだけ同期する（例: "INBOX"）。
    pub only_folder: Option<String>,
    /// この日時以降のメールだけ取る。`None` なら日付で絞らない。
    pub since: Option<DateTime<Utc>>,
    /// raw .eml の保存先ルート。既定は "data/mail"。
    pub data_dir: PathBuf,
    /// 進捗の通知先。`None` なら通知しない。
    ///
    /// フォルダ開始時、フォルダ内で `PROGRESS_EVERY` 通処理するごと、
    /// フォルダの最後の 1 通のあと、そしてアカウント全体の同期が終わったとき
    /// （`done: true`、1 回だけ）に呼ばれる。フォルダ単位の件数はそのフォルダの
    /// 中だけのカウントで、`SyncReport`（全フォルダの累計）とは別物。
    /// `sync_once` が `list_folders` の失敗などで `Err` を返す経路では
    /// `done: true` の通知は来ない（呼び出し側は `Err` そのもので終了を判断する）。
    pub progress: Option<ProgressSink>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            only_folder: None,
            since: Some(Utc::now() - Duration::days(90)),
            data_dir: PathBuf::from("data/mail"),
            progress: None,
        }
    }
}

/// `opts.progress` が設定されていれば通知する。`None` なら何もしない。
fn emit(opts: &SyncOptions, progress: SyncProgress) {
    if let Some(sink) = &opts.progress {
        sink(progress);
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// サーバから取得した通数。
    pub fetched: usize,
    /// 新規に DB へ入れた通数。
    pub inserted: usize,
    /// 既に DB にあった（UID 重複）通数。
    pub skipped: usize,
    /// パース/保存に失敗した通数（フォルダ単位の失敗も 1 件として数える）。
    pub errors: usize,
}

/// 1 通の保存結果。
enum InsertOutcome {
    Inserted,
    Skipped,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// 1 アカウントを 1 回だけ同期する（デーモン化は呼び出し側）。
    ///
    /// `opts.progress` が設定されていれば、フォルダごとの途中経過に加えて、
    /// すべてのフォルダを処理し終えたあと `done: true` の通知を 1 回だけ出す。
    /// ただし `list_folders` の失敗などでこの関数が `Err` を返す場合、
    /// `done: true` の通知は出さない（呼び出し側は戻り値の `Err` で判断する）。
    pub async fn sync_once(
        &self,
        account_id: i64,
        backend: &dyn MailBackend,
        opts: &SyncOptions,
    ) -> Result<SyncReport> {
        let folders = backend.list_folders().await?;
        let folders: Vec<_> = match &opts.only_folder {
            Some(only) => folders.into_iter().filter(|(p, _)| p == only).collect(),
            None => folders,
        };

        let mut report = SyncReport::default();
        for (path, role) in folders {
            if let Err(_err) = self
                .sync_folder(account_id, &path, role, backend, opts, &mut report)
                .await
            {
                tracing::warn!(folder = %path, "folder sync failed");
                report.errors += 1;
            }
        }

        emit(
            opts,
            SyncProgress {
                folder: String::new(),
                fetched: report.fetched,
                total: report.fetched,
                inserted: report.inserted,
                errors: report.errors,
                done: true,
            },
        );

        Ok(report)
    }

    async fn sync_folder(
        &self,
        account_id: i64,
        path: &str,
        role: mailcore::FolderRole,
        backend: &dyn MailBackend,
        opts: &SyncOptions,
        report: &mut SyncReport,
    ) -> Result<()> {
        let folder_id = self.store.ensure_folder(account_id, path, role_str(role))?;

        let status = backend.folder_status(path).await?;
        let stored_uidvalidity = self.store.folder_uidvalidity(folder_id)?;
        if let Some(stored) = stored_uidvalidity {
            if stored != status.uidvalidity {
                tracing::info!(folder = %path, "uidvalidity changed, resyncing folder");
                self.store.reset_folder_uid(folder_id)?;
            }
        }
        self.store
            .set_folder_uidvalidity(folder_id, status.uidvalidity)?;

        let last_uid = self.store.folder_last_uid(folder_id)?;
        let raws = backend.fetch_new(path, last_uid, opts.since).await?;
        tracing::info!(folder = %path, count = raws.len(), "fetched messages");

        let total = raws.len();
        emit(
            opts,
            SyncProgress {
                folder: path.to_string(),
                fetched: 0,
                total,
                inserted: 0,
                errors: 0,
                done: false,
            },
        );

        let mut folder_fetched = 0usize;
        let mut folder_inserted = 0usize;
        let mut folder_errors = 0usize;

        for raw in raws {
            report.fetched += 1;
            folder_fetched += 1;
            let uid = raw.uid;
            // `store.set_folder_last_uid` は MAX(last_uid, uid) で進むので、この UID の
            // 保存に失敗しても後続の UID が成功すれば last_uid はそれを追い越す。つまり
            // この UID は次回以降 fetch_new の範囲から外れ、二度と取得されない。
            // これは意図した挙動: 失敗 UID で last_uid を止めると、恒久的にパースできない
            // 1 通がフォルダ全体の同期を永久に止めてしまう（poison message の方が影響が
            // 大きい）。raw .eml はパース前に保存済みなので本文自体は失われない。
            // TODO(P4): 失敗した UID を記録して再インデックスできるようにする
            match self.store_raw_message(account_id, folder_id, path, &raw, opts) {
                Ok(InsertOutcome::Inserted) => {
                    report.inserted += 1;
                    folder_inserted += 1;
                }
                Ok(InsertOutcome::Skipped) => report.skipped += 1,
                Err(_err) => {
                    let raw_path = opts
                        .data_dir
                        .join(account_id.to_string())
                        .join(sanitize_folder(path))
                        .join(format!("{uid}.eml"));
                    tracing::warn!(
                        folder = %path,
                        uid,
                        raw_path = %raw_path.display(),
                        "failed to parse or store message"
                    );
                    report.errors += 1;
                    folder_errors += 1;
                }
            }

            if folder_fetched.is_multiple_of(PROGRESS_EVERY) || folder_fetched == total {
                emit(
                    opts,
                    SyncProgress {
                        folder: path.to_string(),
                        fetched: folder_fetched,
                        total,
                        inserted: folder_inserted,
                        errors: folder_errors,
                        done: false,
                    },
                );
            }
        }
        Ok(())
    }

    /// raw を保存 → パース → DB へ挿入。1 通ぶんの処理。
    fn store_raw_message(
        &self,
        account_id: i64,
        folder_id: i64,
        folder_path: &str,
        raw: &RawMessage,
        opts: &SyncOptions,
    ) -> Result<InsertOutcome> {
        let dir = opts
            .data_dir
            .join(account_id.to_string())
            .join(sanitize_folder(folder_path));
        std::fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("{}.eml", raw.uid));
        std::fs::write(&file_path, &raw.raw)?;
        let raw_path = file_path.to_string_lossy().into_owned();

        let parsed = parse::parse(&raw.raw)?;

        let thread_key = parse::thread_key(&parsed);
        let snippet = parse::snippet(&parsed.body_text, 120);
        let from = parsed.from.clone().unwrap_or_else(|| Address {
            name: None,
            email: String::new(),
        });
        let date = parsed.date.unwrap_or_else(Utc::now);
        let is_read = has_flag(&raw.flags, "\\Seen");
        let is_flagged = has_flag(&raw.flags, "\\Flagged");

        let new_message = NewMessage {
            account_id,
            folder_id,
            uid: raw.uid,
            message_id: parsed.message_id.as_deref(),
            thread_key: &thread_key,
            from: &from,
            to: &parsed.to,
            cc: &parsed.cc,
            subject: &parsed.subject,
            date,
            snippet: &snippet,
            body_text: &parsed.body_text,
            body_html: parsed.body_html.as_deref(),
            has_attachments: parsed.has_attachments,
            is_read,
            is_flagged,
            raw_path: Some(&raw_path),
        };

        match self.store.insert_message(&new_message)? {
            Some(message_id) => {
                for att in &parsed.attachments {
                    self.store.insert_attachment_meta(
                        message_id,
                        &att.filename,
                        &att.mime,
                        att.size,
                    )?;
                }
                Ok(InsertOutcome::Inserted)
            }
            None => Ok(InsertOutcome::Skipped),
        }
    }
}

fn role_str(role: mailcore::FolderRole) -> &'static str {
    use mailcore::FolderRole::*;
    match role {
        Inbox => "inbox",
        Sent => "sent",
        Drafts => "drafts",
        Trash => "trash",
        Archive => "archive",
        Other => "other",
    }
}

fn has_flag(flags: &[String], flag: &str) -> bool {
    flags.iter().any(|f| f.eq_ignore_ascii_case(flag))
}

/// フォルダ名をファイルパスの 1 セグメントとして安全に使える形にする。
/// `/ \ : * ? " < > |` と制御文字を `_` に置換する。パス区切りや予約文字を
/// 含まないフォルダ名（日本語含む）はそのまま通す。
///
/// フォルダ名は IMAP サーバ由来で信頼境界の外にある。置換後の結果が
/// `"."` / `".."`（カレント/親ディレクトリ）や空文字列になる場合、そのまま
/// パスセグメントとして使うと `data_dir` の外に書き込めてしまうため、
/// 安全な別名に潰す。
fn sanitize_folder(path: &str) -> String {
    let replaced: String = path
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    match replaced.as_str() {
        "" => "_".to_string(),
        "." => "_".to_string(),
        ".." => "__".to_string(),
        _ => replaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::{AccountKind, BackendError, FolderRole, FolderStatus};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    const UTF8_ALT: &[u8] = include_bytes!("../tests/fixtures/utf8-alternative.eml");
    const REPLY_MULTI: &[u8] = include_bytes!("../tests/fixtures/reply-multiprefix.eml");
    const HTML_ONLY: &[u8] = include_bytes!("../tests/fixtures/html-only.eml");

    /// テスト用のフェイクバックエンド。単一フォルダ (INBOX) だけを持つ。
    struct FakeBackend {
        uidvalidity: AtomicU32,
        messages: Vec<(u32, Vec<u8>)>,
        last_since_uid: Mutex<Option<u32>>,
    }

    impl FakeBackend {
        fn new(messages: Vec<(u32, Vec<u8>)>) -> Self {
            Self {
                uidvalidity: AtomicU32::new(100),
                messages,
                last_since_uid: Mutex::new(None),
            }
        }
    }

    #[async_trait::async_trait]
    impl MailBackend for FakeBackend {
        async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError> {
            Ok(vec![("INBOX".to_string(), FolderRole::Inbox)])
        }

        async fn folder_status(&self, _folder: &str) -> Result<FolderStatus, BackendError> {
            Ok(FolderStatus {
                uidvalidity: self.uidvalidity.load(Ordering::SeqCst),
                uid_next: 0,
            })
        }

        async fn fetch_new(
            &self,
            _folder: &str,
            since_uid: u32,
            _since: Option<DateTime<Utc>>,
        ) -> Result<Vec<RawMessage>, BackendError> {
            *self.last_since_uid.lock().unwrap() = Some(since_uid);
            Ok(self
                .messages
                .iter()
                .filter(|(uid, _)| *uid > since_uid)
                .map(|(uid, raw)| RawMessage {
                    uid: *uid,
                    flags: vec!["\\Seen".to_string()],
                    raw: raw.clone(),
                })
                .collect())
        }
    }

    // `SyncEngine::store` は `Arc<Store>` 固定（呼び出し側の要求）。テストの
    // インメモリ DB はスレッドをまたがないので false positive。
    #[allow(clippy::arc_with_non_send_sync)]
    fn setup() -> (Arc<Store>, i64) {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let account = store
            .add_account(
                "test",
                AccountKind::Imap,
                "me@example.com",
                None,
                &serde_json::json!({}),
            )
            .unwrap();
        (store, account.id)
    }

    #[tokio::test]
    async fn initial_sync_inserts_messages_and_saves_eml() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, REPLY_MULTI.to_vec()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: None,
        };

        let report = engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                fetched: 3,
                inserted: 3,
                skipped: 0,
                errors: 0,
            }
        );

        for uid in [1, 2, 3] {
            let path = tmp
                .path()
                .join(account_id.to_string())
                .join("INBOX")
                .join(format!("{uid}.eml"));
            assert!(path.exists(), "expected {path:?} to exist");
        }

        let hits = store
            .search(&mailstore::SearchQuery {
                text: Some("ご案内"),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn sync_reports_progress_and_finishes_with_done() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, REPLY_MULTI.to_vec()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let events: Arc<Mutex<Vec<SyncProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: Some(Arc::new(move |p| sink_events.lock().unwrap().push(p))),
        };

        let report = engine.sync_once(account_id, &backend, &opts).await.unwrap();

        let events = events.lock().unwrap();
        let first = events.first().expect("expected at least one event");
        assert_eq!(first.fetched, 0);
        assert_eq!(first.total, 3);

        let last = events.last().expect("expected at least one event");
        assert!(last.done);
        assert_eq!(last.inserted, report.inserted);

        let done_count = events.iter().filter(|e| e.done).count();
        assert_eq!(done_count, 1);

        for e in events.iter().filter(|e| !e.done) {
            assert_eq!(e.folder, "INBOX");
        }
    }

    #[tokio::test]
    async fn sync_reports_progress_for_an_empty_folder() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(Vec::new());
        let tmp = tempfile::TempDir::new().unwrap();
        let events: Arc<Mutex<Vec<SyncProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: Some(Arc::new(move |p| sink_events.lock().unwrap().push(p))),
        };

        engine.sync_once(account_id, &backend, &opts).await.unwrap();

        let events = events.lock().unwrap();
        let start = events
            .iter()
            .find(|e| !e.done)
            .expect("expected a folder-start event");
        assert_eq!(start.total, 0);
        assert_eq!(start.fetched, 0);

        // 途中通知が出ないこと: `done == false` のイベントは開始通知の 1 件だけ。
        let non_done_count = events.iter().filter(|e| !e.done).count();
        assert_eq!(non_done_count, 1);

        let done_count = events.iter().filter(|e| e.done).count();
        assert_eq!(done_count, 1);
    }

    #[tokio::test]
    async fn progress_is_optional() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, REPLY_MULTI.to_vec()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            data_dir: tmp.path().to_path_buf(),
            ..SyncOptions::default()
        };

        let report = engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(report.inserted, 3);
    }

    #[tokio::test]
    async fn second_sync_resumes_from_last_uid() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, REPLY_MULTI.to_vec()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: None,
        };

        let first = engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(first.inserted, 3);

        let second = engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(second.inserted, 0);
        assert_eq!(*backend.last_since_uid.lock().unwrap(), Some(3));

        let folder_id = store.ensure_folder(account_id, "INBOX", "inbox").unwrap();
        assert_eq!(store.folder_last_uid(folder_id).unwrap(), 3);
    }

    #[tokio::test]
    async fn uidvalidity_change_resets_and_resyncs_from_zero() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, REPLY_MULTI.to_vec()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: None,
        };

        engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(*backend.last_since_uid.lock().unwrap(), Some(0));

        backend.uidvalidity.store(200, Ordering::SeqCst);
        engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(*backend.last_since_uid.lock().unwrap(), Some(0));
    }

    #[tokio::test]
    async fn parse_failure_does_not_stop_sync() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        // 2 通目は空バイト列。`parse::parse` は空入力を必ず失敗として返す。
        let backend = FakeBackend::new(vec![
            (1, UTF8_ALT.to_vec()),
            (2, Vec::new()),
            (3, HTML_ONLY.to_vec()),
        ]);
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: None,
        };

        let report = engine.sync_once(account_id, &backend, &opts).await.unwrap();
        assert_eq!(report.fetched, 3);
        assert_eq!(report.inserted, 2);
        assert_eq!(report.errors, 1);
    }

    #[test]
    fn sanitize_folder_keeps_safe_names() {
        assert_eq!(sanitize_folder("INBOX.送信済み"), "INBOX.送信済み");
    }

    #[test]
    fn sanitize_folder_replaces_reserved_characters() {
        assert_eq!(sanitize_folder("INBOX/Sent"), "INBOX_Sent");
        assert_eq!(sanitize_folder("a\\b:c*d?e\"f<g>h|i"), "a_b_c_d_e_f_g_h_i");
    }

    #[test]
    fn sanitize_folder_rejects_dot_only_names() {
        assert_ne!(sanitize_folder(".."), "..");
        assert_ne!(sanitize_folder("."), ".");
        assert_ne!(sanitize_folder(""), "");
    }

    /// フェイクバックエンドが `".."` という名前のフォルダを `LIST` で返しても、
    /// `.eml` が `data_dir/<account_id>/` の外に出ないことを確かめる。
    struct DotDotBackend {
        message: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl MailBackend for DotDotBackend {
        async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError> {
            Ok(vec![("..".to_string(), FolderRole::Other)])
        }

        async fn folder_status(&self, _folder: &str) -> Result<FolderStatus, BackendError> {
            Ok(FolderStatus {
                uidvalidity: 1,
                uid_next: 0,
            })
        }

        async fn fetch_new(
            &self,
            _folder: &str,
            since_uid: u32,
            _since: Option<DateTime<Utc>>,
        ) -> Result<Vec<RawMessage>, BackendError> {
            if since_uid >= 1 {
                return Ok(Vec::new());
            }
            Ok(vec![RawMessage {
                uid: 1,
                flags: vec![],
                raw: self.message.clone(),
            }])
        }
    }

    #[tokio::test]
    async fn dot_dot_folder_name_does_not_escape_data_dir() {
        let (store, account_id) = setup();
        let engine = SyncEngine::new(store.clone());
        let backend = DotDotBackend {
            message: UTF8_ALT.to_vec(),
        };
        let tmp = tempfile::TempDir::new().unwrap();
        let opts = SyncOptions {
            only_folder: None,
            since: None,
            data_dir: tmp.path().to_path_buf(),
            progress: None,
        };

        engine.sync_once(account_id, &backend, &opts).await.unwrap();

        // data_dir 直下に .eml が出ていないこと（親ディレクトリへ抜けていないこと）。
        for entry in std::fs::read_dir(tmp.path()).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.ends_with(".eml"),
                "found .eml directly under data_dir: {name}"
            );
        }

        // .eml は data_dir/<account_id>/ の下に（サニタイズされたフォルダ名で）出ていること。
        let account_dir = tmp.path().join(account_id.to_string());
        let mut found = false;
        for entry in walk(&account_dir) {
            if entry.extension().and_then(|e| e.to_str()) == Some("eml") {
                found = true;
                assert!(entry.starts_with(&account_dir));
            }
        }
        assert!(found, "expected a .eml under {account_dir:?}");
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.extend(walk(&path));
                } else {
                    out.push(path);
                }
            }
        }
        out
    }
}
