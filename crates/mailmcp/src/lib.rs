//! mailmcp — Claude から見える面。
//!
//! 設計原則:
//! - **1 通ずつ取らせない。** スレッド単位・ダイジェスト単位で返す。
//! - 返り値は軽量（id / 件名 / 差出人 / 日付 / snippet）。本文が要るときだけ `get_thread`。
//! - **送信ツールは出さない。** `create_draft` までで、送信は UI の承認ボタンのみ。
//!
//! ツール一覧（DESIGN.md §6 と同期を取ること）:
//!   list_accounts, search_messages, get_thread, get_message, get_attachment,
//!   inbox_digest, save_summary, upsert_tasks, list_tasks, create_draft, mark
//!
//! TODO(P1): rmcp で実装。ここではツール入出力の型だけ先に固定しておく。

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SearchMessagesArgs {
    pub query: Option<String>,
    pub account_id: Option<i64>,
    pub project: Option<String>,
    /// RFC3339
    pub since: Option<String>,
    #[serde(default)]
    pub unread_only: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    30
}

#[derive(Debug, Deserialize)]
pub struct GetThreadArgs {
    pub thread_key: String,
    #[serde(default)]
    pub include_quotes: bool,
}

#[derive(Debug, Deserialize)]
pub struct InboxDigestArgs {
    pub project: Option<String>,
    /// RFC3339。省略時は直近 24 時間。
    pub since: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveSummaryArgs {
    /// "message:<id>" | "thread:<key>" | "daily:<yyyy-mm-dd>"
    pub target: String,
    pub model: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertTasksArgs {
    pub tasks: Vec<TaskInput>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskInput {
    pub account_id: i64,
    pub source_message_id: Option<i64>,
    pub title: String,
    pub due: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

#[derive(Debug, Deserialize)]
pub struct CreateDraftArgs {
    pub account_id: i64,
    pub in_reply_to: Option<i64>,
    pub to: Option<Vec<String>>,
    pub subject: Option<String>,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct MarkArgs {
    pub ids: Vec<i64>,
    /// "read" | "unread" | "archive" | "flag" | "unflag"
    pub action: String,
}
