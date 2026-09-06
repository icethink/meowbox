/**
 * `types.ts` は mailcore のドメイン型、ここは `apps/desktop/src-tauri/src/commands/` の
 * DTO を写したもの。Rust 側を変えたらここも直す。
 *
 * フィールド名は Rust 側の serde 表現に合わせる。`NewAccountInput` / `ThreadFilter` は
 * `#[serde(rename_all = "snake_case")]` が付いているので snake_case のまま。
 * i64 / usize は number、`Option<T>` は `T | null`（`#[serde(default)]` が付いている
 * `ThreadFilter` のフィールドだけ省略可能 = `?`）、`DateTime<Utc>` と RFC 3339 の
 * `String` は `Timestamp`（= string）として扱う。
 */

import type { AccountKind, Address, Task, Timestamp } from './types';

// --- commands/accounts.rs -------------------------------------------------

/** ウィザードから来るアカウント設定。パスワードは含まない */
export interface NewAccountInput {
  name: string;
  kind: AccountKind;
  email: string;
  project_tag: string | null;
  host: string;
  port: number;
  username: string;
  starttls: boolean;
}

/** 接続テストの結果 */
export interface TestConnectionResult {
  /** 見えたフォルダ名（先頭 20 件まで） */
  folders: string[];
}

/** サイドバーの案件グループ */
export interface ProjectGroup {
  /** project_tag。未設定のアカウントは null のグループにまとまる */
  tag: string | null;
  accounts: ProjectAccount[];
  unread: number;
}

export interface ProjectAccount {
  id: number;
  email: string;
}

// --- commands/threads.rs ---------------------------------------------------

/** スレッド一覧の絞り込み。`#[serde(default)]` のためすべて省略可能 */
export interface ThreadFilter {
  project_tag?: string | null;
  account_id?: number | null;
  unread_only?: boolean;
  flagged_only?: boolean;
  include_archived?: boolean;
  limit?: number | null;
  offset?: number | null;
}

/** スレッド 1 本ぶん。本文と引用を含む */
export interface ThreadDetailDto {
  thread_key: string;
  /** 最新メッセージの件名 */
  subject: string;
  project_tag: string | null;
  messages: MessageDto[];
  /** Claude が保存した要約（`thread:<key>`）。まだ無ければ null */
  summary: SummaryDto | null;
  /** このスレッドのメッセージから抽出されたタスク。まだ無ければ空 */
  tasks: Task[];
}

export interface MessageDto {
  id: number;
  account_id: number;
  thread_key: string;
  from: Address;
  to: Address[];
  cc: Address[];
  subject: string;
  /** RFC 3339。表示用のラベルは UI が作る */
  date: Timestamp;
  /** 引用・署名を落とした本文（DB の body_text） */
  body_text: string;
  /** raw .eml から読み直した引用・署名部分。読めなければ空文字列 */
  quoted_text: string;
  has_attachments: boolean;
  is_read: boolean;
  is_flagged: boolean;
  attachments: AttachmentDto[];
}

export interface AttachmentDto {
  id: number;
  filename: string;
  mime: string;
  size: number;
}

export interface SummaryDto {
  target: string;
  model: string;
  summary: string;
  created_at: Timestamp;
}

/** サイドバーの「ダイジェスト」に出す 1 案件ぶんのグループ */
export interface DigestGroupDto {
  project_tag: string | null;
  tasks: Task[];
}

export interface DigestDto {
  /** Claude が保存した日次要約（`daily:<date_key>`）。まだ無ければ null */
  summary: SummaryDto | null;
  groups: DigestGroupDto[];
}

/** サイドバーの「ビュー」に出す件数 */
export interface ViewCountsDto {
  all: number;
  unread: number;
  flagged: number;
  tasks: number;
  drafts: number;
}

// --- commands/drafts.rs ------------------------------------------------------

/** 返信下書きの作成入力。返信元のメッセージから宛先・件名を決める */
export interface NewDraftInput {
  /** 返信元のメッセージ id。これが下書きのアカウント・宛先・件名の元になる */
  in_reply_to: number;
  body: string;
}

export interface DraftDto {
  id: number;
  account_id: number;
  in_reply_to: number | null;
  to: Address[];
  subject: string;
  body: string;
  /** 常に "draft"。送信は UI の承認操作だけ（MCP にも送信ツールを出さない） */
  status: string;
  created_at: Timestamp;
}

// --- commands/sync.rs --------------------------------------------------------

/** `sync://progress` で流れる進捗。件数とフォルダ名だけで本文・パスワードは含まない */
export interface SyncProgressEvent {
  account_id: number;
  /** いま処理しているフォルダ。完了通知では空文字列 */
  folder: string;
  fetched: number;
  /** `fetched` の分母 */
  total: number;
  inserted: number;
  errors: number;
  /** アカウント 1 回分の同期が終わったら true */
  done: boolean;
  /** 同期が失敗したときだけ入る日本語メッセージ */
  error: string | null;
}

/** アカウントごとの直近の同期結果 */
export interface LastSync {
  /** RFC 3339 */
  finished_at: Timestamp;
  inserted: number;
  errors: number;
  /** 失敗したときだけ */
  error: string | null;
}

// --- commands/mcp.rs ---------------------------------------------------------

/** Claude に登録するための情報。パスとコピペ用の文字列だけを返す */
export interface McpIntegration {
  /** meowbox-mcp の絶対パス */
  server_path: string;
  /** そのパスに実行ファイルが実在するか。false ならまだビルドされていない（開発中など） */
  server_exists: boolean;
  /** claude_desktop_config.json に貼る JSON */
  desktop_config_json: string;
  /** Claude Code / Cowork 用のコマンド 1 行 */
  claude_code_command: string;
}

// --- error.rs ----------------------------------------------------------------

/** invoke が reject したときの中身 */
export interface AppError {
  /** UI が分岐に使う識別子。"not_found" | "invalid_input" | "conflict" | "auth" | "network" | "internal" */
  code: string;
  /** 人間に見せる日本語メッセージ */
  message: string;
}
