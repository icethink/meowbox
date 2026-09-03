//! 汎用 IMAP バックエンド。
//!
//! TODO(P0):
//! - `async-imap` で接続（STARTTLS / TLS）。認証は PLAIN か XOAUTH2。
//! - `list_folders`: LIST + SPECIAL-USE 属性で role を推定（\Sent, \Drafts, \Trash, \Archive）。
//! - `fetch_new`: `UID FETCH (since_uid+1):* (FLAGS RFC822)` を 200 件ずつ。
//! - UIDVALIDITY 変化時は folder を丸ごと再同期する（engine 側で判定）。
//! - パスワード / トークンは `keyring` から取得し、この構造体には持たない。

use mailcore::{BackendError, FolderRole, MailBackend, RawMessage};

#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub starttls: bool,
}

pub struct ImapBackend {
    pub config: ImapConfig,
}

impl ImapBackend {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl MailBackend for ImapBackend {
    async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError> {
        Err(BackendError::Protocol(
            "ImapBackend::list_folders is not implemented yet (P0)".into(),
        ))
    }

    async fn fetch_new(
        &self,
        _folder: &str,
        _since_uid: u32,
    ) -> Result<Vec<RawMessage>, BackendError> {
        Err(BackendError::Protocol(
            "ImapBackend::fetch_new is not implemented yet (P0)".into(),
        ))
    }
}
