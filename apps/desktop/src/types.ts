/**
 * `crates/mailcore/src/lib.rs` のドメイン型に 1:1 で対応する TypeScript 型。
 *
 * フィールド名と enum の値は Rust 側の serde 表現（`rename_all = "snake_case"`）に
 * 合わせてある。P3 で `invoke()` の戻り値をそのまま流し込めるようにするため、
 * ここに UI 都合の項目を足さないこと（派生型は `types.ui.ts`）。
 *
 * i64 は number（メール ID が 2^53 を超えることはない）、
 * `DateTime<Utc>` は RFC 3339 文字列として扱う。
 */

export type AccountId = number;
export type MessageId = number;
/** RFC 3339 / ISO 8601 の日時文字列 */
export type Timestamp = string;

/** mailcore::AccountKind */
export type AccountKind = 'imap' | 'gmail' | 'm365';

/** mailcore::FolderRole */
export type FolderRole = 'inbox' | 'sent' | 'drafts' | 'trash' | 'archive' | 'other';

/** mailcore::TaskStatus */
export type TaskStatus = 'open' | 'done' | 'dismissed';

/** mailcore::Task.created_by / mailcore::Draft.status は Rust 側が String なので広めに取る */
export type CreatedBy = 'ai' | 'user';
export type DraftStatus = 'draft' | 'approved' | 'sent';

/** mailcore::Address */
export interface Address {
  name: string | null;
  email: string;
}

/** mailcore::Account */
export interface Account {
  id: AccountId;
  name: string;
  kind: AccountKind;
  email: string;
  project_tag: string | null;
  /** バックエンド固有設定。秘密情報は入らない（keyring 側に持つ） */
  settings: Record<string, unknown>;
  created_at: Timestamp;
}

/** mailcore::Message */
export interface Message {
  id: MessageId;
  account_id: AccountId;
  folder_path: string;
  uid: number;
  message_id: string | null;
  thread_key: string;
  from: Address;
  to: Address[];
  cc: Address[];
  subject: string;
  date: Timestamp;
  snippet: string;
  /** 引用・署名を除いた要約向けテキスト */
  body_text: string;
  has_attachments: boolean;
  is_read: boolean;
  is_flagged: boolean;
}

/** mailcore::MessageSummary — 一覧・検索の戻り値はこれ（本文は含まない） */
export interface MessageSummary {
  id: MessageId;
  account_id: AccountId;
  thread_key: string;
  from: Address;
  subject: string;
  date: Timestamp;
  snippet: string;
  is_read: boolean;
}

/** mailcore::ThreadSummary — スレッド一覧の戻り値。最新メッセージの情報を代表させる */
export interface ThreadSummary {
  thread_key: string;
  /** 最新メッセージのアカウント */
  account_id: AccountId;
  project_tag: string | null;
  /** 最新メッセージの id */
  latest_message_id: MessageId;
  /** 最新メッセージの件名 */
  subject: string;
  /** 最新メッセージの差出人 */
  from: Address;
  snippet: string;
  /** 最新メッセージの日時 */
  last_date: Timestamp;
  message_count: number;
  unread_count: number;
  /** スレッド内に 1 通でも添付があれば true */
  has_attachments: boolean;
  /** スレッド内に 1 通でもフラグがあれば true */
  is_flagged: boolean;
}

/** mailcore::Task — confidence が低いものは UI で「候補」扱いにする */
export interface Task {
  id: number;
  account_id: AccountId;
  source_message_id: MessageId | null;
  title: string;
  due: Timestamp | null;
  status: TaskStatus;
  confidence: number;
  created_by: CreatedBy;
  created_at: Timestamp;
}

/** mailcore::Draft — 送信は UI の承認ボタンからのみ。MCP には送信ツールを出さない */
export interface Draft {
  id: number;
  account_id: AccountId;
  in_reply_to: MessageId | null;
  to: Address[];
  subject: string;
  body: string;
  status: DraftStatus;
  created_at: Timestamp;
}
