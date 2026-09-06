/**
 * `mock.ts` と `tauri.ts` に共通のインターフェース。
 * 両方に `satisfies MeowboxApi` を付けることで、シグネチャの食い違いを
 * TypeScript の型検査で防ぐ。
 */

import type { Account } from '../types';
import type {
  DraftDto,
  LastSync,
  MessageDto,
  NewAccountInput,
  NewDraftInput,
  SyncProgressEvent,
  TestConnectionResult,
} from '../types.api';
import type {
  Digest,
  ProjectGroupView,
  SyncState,
  ThreadDetail,
  ThreadListItem,
  ViewItemView,
  ViewKey,
} from '../types.ui';

/** スレッド一覧の絞り込み。UI 都合の `view` はここで受け、`unread_only` 等への変換は各実装が行う */
export interface ThreadQuery {
  /** 案件タグで絞る。null / 未指定は全案件 */
  project_tag?: string | null;
  /** アカウントで絞る */
  account_id?: number | null;
  /** サイドバーのビュー */
  view?: ViewKey;
  limit?: number;
  offset?: number;
}

/** `mark` に渡せる操作。既読・フラグ・アーカイブの付け外し */
export type MarkAction = 'read' | 'unread' | 'flag' | 'unflag' | 'archive' | 'unarchive';

export interface MeowboxApi {
  /** 送信が実装済みか。MVP では常に false（下書きの保存まで。CLAUDE.md の安全境界） */
  sendAvailable: boolean;
  /** Claude の下書き生成が使えるか。P1 で MCP 経由に繋ぐまで false */
  aiDraftAvailable: boolean;

  listAccounts(): Promise<Account[]>;
  addAccount(input: NewAccountInput): Promise<Account>;
  setAccountPassword(id: number, password: string): Promise<void>;
  testConnection(input: NewAccountInput, password: string): Promise<TestConnectionResult>;
  deleteAccount(id: number): Promise<void>;

  listProjects(): Promise<ProjectGroupView[]>;
  listViews(): Promise<ViewItemView[]>;

  listThreads(query?: ThreadQuery): Promise<ThreadListItem[]>;
  getThread(threadKey: string): Promise<ThreadDetail | null>;
  getMessage(id: number): Promise<MessageDto | null>;
  extractAttachment(id: number): Promise<string>;
  /** 添付を展開して OS の既定アプリで開く。パスの検証は Rust 側で行う */
  openAttachment(id: number): Promise<void>;

  getDigest(startOfDay?: Date): Promise<Digest>;
  mark(ids: number[], action: MarkAction): Promise<number>;

  syncAccount(id: number): Promise<void>;
  listSyncStatus(): Promise<Record<number, LastSync>>;
  onSyncProgress(cb: (e: SyncProgressEvent) => void): Promise<() => void>;
  getSyncStatus(): Promise<{ state: SyncState; label: string }>;

  /**
   * 返信下書きを保存する。**送信はしない**（MVP の安全境界）。
   * MCP 側にも送信ツールは出さず、送信は UI の「確認して送信」だけが行う。
   */
  createDraft(input: NewDraftInput): Promise<DraftDto>;
  listDrafts(): Promise<DraftDto[]>;
  /** Claude に返信下書きを書かせる。P3 で MCP / Claude API に繋ぐ */
  generateAiDraft(threadKey: string): Promise<string>;
  /** 「確認して送信」。人間が承認したときだけ呼ばれる */
  sendDraft(input: { thread_key: string; body: string }): Promise<void>;
}
