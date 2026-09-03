//! 同期エンジン。アカウントごとに 1 タスク。
//!
//! TODO(P0):
//! 1. `list_folders` → `Store::ensure_folder`
//! 2. 各フォルダについて `folder_last_uid` を読み、`fetch_new(folder, last_uid)`
//! 3. `parse::parse` → `NewMessage` → `Store::insert_message`
//! 4. raw を `data/mail/<account>/<folder>/<uid>.eml` に保存し `raw_path` に入れる
//! 5. 初回は直近 90 日だけ取り、残りはバックグラウンドで遡る
//!
//! TODO(P4): INBOX は IMAP IDLE、他は 5〜15 分ポーリング。

use std::sync::Arc;

use anyhow::Result;
use mailcore::MailBackend;
use mailstore::Store;

pub struct SyncEngine {
    pub store: Arc<Store>,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// 1 アカウントを 1 回だけ同期する（デーモン化は呼び出し側）。
    pub async fn sync_once(&self, account_id: i64, backend: &dyn MailBackend) -> Result<SyncReport> {
        let folders = backend.list_folders().await?;
        let mut report = SyncReport::default();
        for (path, role) in folders {
            let folder_id = self.store.ensure_folder(account_id, &path, role_str(role))?;
            let last_uid = self.store.folder_last_uid(folder_id)?;
            let raws = backend.fetch_new(&path, last_uid).await?;
            for raw in raws {
                // TODO(P0): parse + insert。今はカウントだけ。
                let _ = raw;
                report.fetched += 1;
            }
        }
        Ok(report)
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

#[derive(Debug, Default)]
pub struct SyncReport {
    pub fetched: usize,
    pub inserted: usize,
}
