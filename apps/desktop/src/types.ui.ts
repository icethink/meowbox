/**
 * UI 専用の派生型。`types.ts`（= Rust のドメイン型）とは混ぜない。
 *
 * ここに置くのは「表示するために組み立てたもの」だけ。DB に無い項目
 * （相対時刻のラベル、要約の強調範囲、確度のしきい値判定など）はここで持つ。
 */

import type { AccountId, Address, Task, Timestamp } from './types';

/** サイドバーの左端に出す状態ドット */
export type SyncState = 'ok' | 'syncing' | 'warn' | 'error';

/** サイドバー「ビュー」の識別子 */
export type ViewKey = 'all' | 'unread' | 'action' | 'flagged' | 'tasks' | 'drafts';

/** 一覧行の左アクセントバー。人間側の緊急度なので --accent / --danger を使う */
export type Urgency = 'urgent' | 'action' | null;

/** 案件（project_tag）でまとめたサイドバーの 1 グループ */
export interface ProjectGroupView {
  tag: string;
  /** 所属アカウント。1 案件に複数アドレスが配布されることがある */
  accounts: { id: AccountId; email: string }[];
  unread: number;
  /** 要対応スレッドを抱えているか。true のときだけ件数を --accent で強調する */
  has_action: boolean;
  sync: SyncState;
}

export interface ViewItemView {
  key: ViewKey;
  label: string;
  count: number;
  /** 件数を --accent / --ai のどちらで見せるか。null は無彩色 */
  tone: 'accent' | 'ai' | null;
}

/** 添付のチップ表示に必要な分だけ */
export interface AttachmentChip {
  id: number;
  filename: string;
  /** "18KB" のような表示用文字列 */
  size_label: string;
}

/** スレッド一覧の 1 行 */
export interface ThreadListItem {
  thread_key: string;
  account_id: AccountId;
  project_tag: string;
  from_name: string;
  /** 件名。先頭の【至急】などは表示側で切り出して --danger にする */
  subject: string;
  /** Claude が付けた 1 行要約。--ai で表示する */
  ai_snippet: string | null;
  /** Claude の要約が無いときに出す本文の抜粋 */
  snippet: string;
  /** "10:24" / "昨日" / "8/29" のような表示用ラベル */
  time_label: string;
  date: Timestamp;
  is_unread: boolean;
  urgency: Urgency;
  has_attachments: boolean;
  task_count: number;
  /** GitLab の通知メールのように、読まなくてよいものを薄く出す */
  is_muted: boolean;
}

/** 要約本文の強調範囲。HTML を流し込まずに済むよう区間で持つ */
export interface RichSpan {
  text: string;
  strong?: boolean;
}

/** Claude が生成した要約 */
export interface AiSummary {
  target: string;
  body: RichSpan[];
  /** "10:02" のような表示用ラベル */
  generated_label: string;
  generated_at: Timestamp;
}

/** スレッド内の 1 通 */
export interface ThreadMessageView {
  id: number;
  from: Address;
  /** アバターに出す 1 文字 */
  initial: string;
  /** "9/1 17:20" / "今日 9:41" */
  time_label: string;
  body: RichSpan[];
  /** 折り畳んでいる引用の行数。0 なら引用なし */
  quoted_lines: number;
  quoted_text: string;
  attachments: AttachmentChip[];
  /** 最新の 1 通は本文を明るく出す */
  is_latest: boolean;
}

/** スレッド表示に必要な全部 */
export interface ThreadDetail {
  thread_key: string;
  subject: string;
  project_tag: string;
  /** ヘッダの「◯◯ <mail> との N 通」 */
  counterpart: Address;
  message_count: number;
  summary: AiSummary | null;
  tasks: Task[];
  messages: ThreadMessageView[];
  /** 返信欄のプレースホルダ（「山田さんへ返信…」） */
  reply_placeholder: string;
}

/** ダイジェストの 1 項目 */
export interface DigestItem {
  id: number;
  title: string;
  /** "今日" / "9/5" / "9/12" */
  due_label: string | null;
  /** 期限の色。人間側の緊急度なので accent / danger */
  due_tone: 'danger' | 'accent' | 'neutral';
  /** confidence が低い AI 候補。点線 + --ai で出す */
  is_candidate: boolean;
}

export interface DigestGroup {
  project_tag: string;
  items: DigestItem[];
}

/** 右パネルまるごと */
export interface Digest {
  /** "9/2 (火)" */
  date_label: string;
  /** Claude のまとめ */
  summary: RichSpan[];
  groups: DigestGroup[];
}
