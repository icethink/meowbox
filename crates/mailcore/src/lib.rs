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
    pub body_html: Option<String>,
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

/// スレッド一覧の 1 行。DB の messages を thread_key で畳んだ結果。
/// 本文は含めない（一覧は軽量に保つ）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub thread_key: String,
    /// 最新メッセージのアカウント
    pub account_id: AccountId,
    pub project_tag: Option<String>,
    /// 最新メッセージの id
    pub latest_message_id: MessageId,
    /// 最新メッセージの件名
    pub subject: String,
    /// 最新メッセージの差出人
    pub from: Address,
    pub snippet: String,
    /// 最新メッセージの日時
    pub last_date: DateTime<Utc>,
    pub message_count: i64,
    pub unread_count: i64,
    /// スレッド内に 1 通でも添付があれば true
    pub has_attachments: bool,
    /// スレッド内に 1 通でもフラグがあれば true
    pub is_flagged: bool,
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

    /// フォルダの UIDVALIDITY と次の UID を返す。
    async fn folder_status(&self, folder: &str) -> Result<FolderStatus, BackendError>;

    /// `since_uid` より新しく、かつ `since` 以降に届いたメッセージを raw (RFC822) で返す。
    /// `since` が `None` なら日付で絞らない。
    async fn fetch_new(
        &self,
        folder: &str,
        since_uid: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<RawMessage>, BackendError>;
}

#[derive(Debug, Clone)]
pub struct RawMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub raw: Vec<u8>,
}

/// フォルダの現在の状態。UIDVALIDITY が前回と変わっていたら
/// UID の意味が変わっているので、そのフォルダは取り直す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderStatus {
    pub uidvalidity: u32,
    pub uid_next: u32,
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

/// 全角 ASCII（U+FF01-FF5E）を半角に畳み込み、ASCII 英字は小文字化する。
/// プレフィックス判定用の比較キーを作るためだけに使う（元の文字列は別途保持する）。
fn fold_char(c: char) -> char {
    let c = match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        other => other,
    };
    c.to_ascii_lowercase()
}

/// `idx` から始まる空白（U+3000 全角スペース含む）を読み飛ばす。
fn skip_ws(chars: &[char], mut idx: usize) -> usize {
    while idx < chars.len() && chars[idx].is_whitespace() {
        idx += 1;
    }
    idx
}

/// `idx` が `[123]` / `(123)` のような連番括弧なら読み飛ばした位置を返す。
/// 括弧でない、または中に数字が無ければ `idx` をそのまま返す。
fn skip_seq_bracket(chars: &[char], idx: usize) -> usize {
    let close = match chars.get(idx) {
        Some('[') => ']',
        Some('(') => ')',
        _ => return idx,
    };
    let digits_start = idx + 1;
    let mut j = digits_start;
    while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
    }
    if j > digits_start && chars.get(j) == Some(&close) {
        j + 1
    } else {
        idx
    }
}

/// `folded`（畳み込み済み・小文字化済み）の `start` 位置に返信/転送プレフィックスが
/// あれば、コロンの次の文字インデックスを返す。
fn match_prefix(folded: &[char], start: usize) -> Option<usize> {
    // "fwd" は "fw" を含むので先に試す。
    const LETTER_PREFIXES: [&str; 3] = ["fwd", "fw", "re"];
    const KANJI_PREFIXES: [&str; 3] = ["返信", "転送", "回答"];

    let try_prefix = |p: &str| -> Option<usize> {
        let p_chars: Vec<char> = p.chars().collect();
        let end = start + p_chars.len();
        if end > folded.len() || folded[start..end] != p_chars[..] {
            return None;
        }
        let after_bracket = skip_seq_bracket(folded, end);
        if folded.get(after_bracket) == Some(&':') {
            Some(after_bracket + 1)
        } else {
            None
        }
    };

    LETTER_PREFIXES
        .iter()
        .chain(KANJI_PREFIXES.iter())
        .find_map(|p| try_prefix(p))
}

/// スレッドキーの近似計算。`References` / `In-Reply-To` が無い場合のフォールバック。
///
/// 先頭の返信/転送プレフィックス（`Re:` `Fw:` `Fwd:` `返信:` `転送:` `回答:`、
/// 大文字小文字・全角/半角・`Re[2]:` `Re(2):` のような連番付きを問わない）を
/// 多重に剥がす。文字境界を壊さないよう、バイト単位ではなく char 単位で処理する。
pub fn normalize_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    let original: Vec<char> = trimmed.chars().collect();
    let folded: Vec<char> = original.iter().map(|&c| fold_char(c)).collect();

    let mut start = 0usize;
    loop {
        start = skip_ws(&folded, start);
        match match_prefix(&folded, start) {
            Some(end) => start = end,
            None => break,
        }
    }
    start = skip_ws(&folded, start);

    original[start..].iter().collect()
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

    #[test]
    fn strips_numbered_prefixes() {
        assert_eq!(normalize_subject("Re[2]: 定例会の件"), "定例会の件");
        assert_eq!(normalize_subject("RE[10]: 週次報告"), "週次報告");
        assert_eq!(normalize_subject("Re(3): 週次報告"), "週次報告");
    }

    #[test]
    fn strips_fullwidth_colon_and_letters() {
        assert_eq!(normalize_subject("Re： 全角コロン"), "全角コロン");
        assert_eq!(normalize_subject("返信：見積の件"), "見積の件");
        assert_eq!(normalize_subject("ＲＥ：全角のRe"), "全角のRe");
    }

    #[test]
    fn strips_mixed_multiple_prefixes() {
        assert_eq!(normalize_subject("Re: FW: 返信： 見積の件"), "見積の件");
    }

    #[test]
    fn strips_japanese_forward_and_reply_prefixes() {
        assert_eq!(normalize_subject("転送: 議事録"), "議事録");
        assert_eq!(normalize_subject("回答: アンケート"), "アンケート");
    }

    #[test]
    fn does_not_strip_lookalike_words() {
        assert_eq!(
            normalize_subject("Reply: これは剥がさない"),
            "Reply: これは剥がさない"
        );
        assert_eq!(normalize_subject("Regarding: 提案"), "Regarding: 提案");
    }

    #[test]
    fn prefix_only_subject_returns_empty() {
        assert_eq!(normalize_subject("Re:"), "");
    }

    /// `MailBackend` がオブジェクト安全で、3 メソッドとも呼べることを確認するフェイク。
    struct FakeBackend;

    #[async_trait::async_trait]
    impl MailBackend for FakeBackend {
        async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError> {
            Ok(vec![("INBOX".into(), FolderRole::Inbox)])
        }

        async fn folder_status(&self, _folder: &str) -> Result<FolderStatus, BackendError> {
            Ok(FolderStatus {
                uidvalidity: 1,
                uid_next: 2,
            })
        }

        async fn fetch_new(
            &self,
            _folder: &str,
            _since_uid: u32,
            _since: Option<DateTime<Utc>>,
        ) -> Result<Vec<RawMessage>, BackendError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn mail_backend_is_object_safe_and_callable() {
        let backend: &dyn MailBackend = &FakeBackend;

        let folders = backend.list_folders().await.unwrap();
        assert_eq!(folders, vec![("INBOX".to_string(), FolderRole::Inbox)]);

        let status = backend.folder_status("INBOX").await.unwrap();
        assert_eq!(
            status,
            FolderStatus {
                uidvalidity: 1,
                uid_next: 2,
            }
        );

        let raws = backend.fetch_new("INBOX", 0, None).await.unwrap();
        assert!(raws.is_empty());
    }
}
