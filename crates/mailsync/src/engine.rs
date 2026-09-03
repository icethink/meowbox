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

pub struct SyncEngine {
    pub store: Arc<Store>,
}

/// `sync_once` の挙動を調整するオプション。
pub struct SyncOptions {
    /// 指定があればこのフォルダだけ同期する（例: "INBOX"）。
    pub only_folder: Option<String>,
    /// この日時以降のメールだけ取る。`None` なら日付で絞らない。
    pub since: Option<DateTime<Utc>>,
    /// raw .eml の保存先ルート。既定は "data/mail"。
    pub data_dir: PathBuf,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            only_folder: None,
            since: Some(Utc::now() - Duration::days(90)),
            data_dir: PathBuf::from("data/mail"),
        }
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

        for raw in raws {
            report.fetched += 1;
            let uid = raw.uid;
            match self.store_raw_message(account_id, folder_id, path, &raw, opts) {
                Ok(InsertOutcome::Inserted) => report.inserted += 1,
                Ok(InsertOutcome::Skipped) => report.skipped += 1,
                Err(_err) => {
                    tracing::warn!(folder = %path, uid, "failed to parse or store message");
                    report.errors += 1;
                }
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
