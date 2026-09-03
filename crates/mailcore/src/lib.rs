//! mailcore — Meowbox のドメインモデル。
//!
//! このクレートは I/O を持たない。IMAP/SMTP/DB/UI のどれにも依存しないので、
//! mailcli / mailmcp / Tauri のどこからでも同じ型と契約を共有できる。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type AccountId = i64;
pub type MessageId = i64;

/// アカウント種別。認証方式とバックエンド実装の選択に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// 汎用 IMAP/SMTP（パスワード認証）
    Imap,
    /// Gmail / Google Workspace（OAuth2, XOAUTH2 over IMAP）
    Gmail,
    /// Microsoft 365（OAuth2, Graph API 優先）
    M365,
}

impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountKind::Imap => "imap",
            AccountKind::Gmail => "gmail",
            AccountKind::M365 => "m365",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "imap" => Some(AccountKind::Imap),
            "gmail" => Some(AccountKind::Gmail),
            "m365" => Some(AccountKind::M365),
            _ => None,
        }
    }
}

/// 案件ごとに配布されるアドレスを束ねる単位。`project_tag` が同じアカウントは
/// UI / MCP 上で 1 つの案件として横断検索できる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub kind: AccountKind,
    pub email: String,
    pub project_tag: Option<String>,
    /// バックエンド固有設定（IMAP host/port, OAuth client id など）。秘密情報は入れない。
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

/// 1 通のメール。`body_text` は引用・署名を除いた要約向けテキスト。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder_path: String,
    pub uid: u32,
    pub message_id: Option<String>,
    pub thread_key: String,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub snippet: String,
    pub body_text: String,
    pub has_attachments: bool,
    pub is_read: bool,
    pub is_flagged: bool,
}

/// 検索結果などで使う軽量ビュー。MCP から返すのは基本これ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_key: String,
    pub from: Address,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub snippet: String,
    pub is_read: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Done,
    Dismissed,
}

/// AI またはユーザーが抽出したタスク。`confidence` が低いものは UI で「候補」扱い。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub account_id: AccountId,
    pub source_message_id: Option<MessageId>,
    pub title: String,
    pub due: Option<DateTime<Utc>>,
    pub status: TaskStatus,
    pub confidence: f32,
    pub created_by: String, // "ai" | "user"
    pub created_at: DateTime<Utc>,
}

/// 返信下書き。MVP では送信は UI からのみ行う（MCP には送信ツールを出さない）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: i64,
    pub account_id: AccountId,
    pub in_reply_to: Option<MessageId>,
    pub to: Vec<Address>,
    pub subject: String,
    pub body: String,
    pub status: String, // "draft" | "approved" | "sent"
    pub created_at: DateTime<Utc>,
}

/// 同期バックエンドの契約。IMAP / Gmail / Graph がこれを実装する。
/// 認証情報の取得は実装側（keyring 等）に任せ、core は知らない。
#[async_trait::async_trait]
pub trait MailBackend: Send + Sync {
    /// フォルダ一覧を返す。
    async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError>;
    /// `since_uid` より新しいメッセージを raw (RFC822) で返す。
    async fn fetch_new(
        &self,
        folder: &str,
        since_uid: u32,
    ) -> Result<Vec<RawMessage>, BackendError>;
}

#[derive(Debug, Clone)]
pub struct RawMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub raw: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// スレッドキーの近似計算。`References` / `In-Reply-To` が無い場合のフォールバック。
pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim();
    loop {
        let lower = s.to_ascii_lowercase();
        let stripped = ["re:", "fw:", "fwd:", "回答:", "返信:", "転送:"]
            .iter()
            .find_map(|p| lower.starts_with(p).then(|| s[p.len()..].trim_start()));
        match stripped {
            Some(rest) if rest.len() < s.len() => s = rest,
            _ => break,
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_reply_prefixes() {
        assert_eq!(normalize_subject("Re: Re: 見積の件"), "見積の件");
        assert_eq!(normalize_subject("FWD: hello"), "hello");
        assert_eq!(normalize_subject("plain"), "plain");
    }
}
