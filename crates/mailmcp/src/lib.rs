//! mailmcp — Claude から見える面。`meowbox-mcp` バイナリ本体は `main.rs`。
//!
//! 設計原則:
//! - **1 通ずつ取らせない。** スレッド単位・ダイジェスト単位で返す。
//! - 返り値は軽量（id / 件名 / 差出人 / 日付 / snippet）。本文が要るときだけ `get_thread`。
//! - **`mark`（既読・アーカイブ）と送信は MCP に出さない**（ADR 0002 / 0007）。
//!
//! ツール一覧（DESIGN.md §6 と同期を取ること）:
//!   list_accounts, search_messages, get_thread, get_message, get_attachment,
//!   inbox_digest, save_summary, upsert_tasks, list_tasks, create_draft
//!
//! 現状: 全ツールを実装済み（`main.rs`）。ここには各ツールの引数の型だけを置く
//! （出力の DTO は `main.rs` 側にある）。

use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// すべてのツール description の末尾に必ず入れる、送信・既読変更をしないことの明記。
/// `main.rs` の `safety_note_ja!` / `safety_note_en!` マクロと文字列が一致することを
/// テストで確認する（`tests::safety_note_constants_match_macros`）。
pub const SAFETY_NOTE_JA: &str = "送信はできません。既読状態は変更されません。";
pub const SAFETY_NOTE_EN: &str = "This server cannot send mail and never changes read state.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
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
    /// true のとき body_text の先頭 2,000 文字を各結果に付ける。
    #[serde(default)]
    pub include_body: bool,
}

fn default_limit() -> usize {
    30
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetThreadArgs {
    pub thread_key: String,
    #[serde(default)]
    pub include_quotes: bool,
}

/// `get_message` の引数。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetMessageArgs {
    pub id: i64,
    /// true のとき `body_html` を返す（元のメールに HTML が無ければ null のまま）。
    /// false のときは常に null。
    #[serde(default)]
    pub include_html: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InboxDigestArgs {
    pub project: Option<String>,
    /// RFC3339。省略時は直近 24 時間。
    pub since: Option<String>,
    /// 返すスレッド数の上限。既定 20、最大 50。
    #[serde(default = "default_digest_limit")]
    pub limit: usize,
}

fn default_digest_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveSummaryArgs {
    /// "message:<id>" | "thread:<key>" | "daily:<yyyy-mm-dd>"
    pub target: String,
    pub model: String,
    pub summary: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpsertTasksArgs {
    pub tasks: Vec<TaskInput>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct TaskInput {
    /// 省略時は `source_message_id` から推定する。
    pub account_id: Option<i64>,
    pub source_message_id: Option<i64>,
    pub title: String,
    pub due: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

/// `list_tasks` の引数。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksArgs {
    /// "open" | "done" | "dismissed"
    pub status: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateDraftArgs {
    pub account_id: i64,
    pub in_reply_to: Option<i64>,
    pub to: Option<Vec<String>>,
    pub subject: Option<String>,
    pub body: String,
    /// true のとき、元メールの差出人に加えて to / cc も宛先にする（自分のアドレスは除く）。
    #[serde(default)]
    pub reply_all: bool,
}

/// `get_attachment` の引数。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAttachmentArgs {
    pub id: i64,
}
