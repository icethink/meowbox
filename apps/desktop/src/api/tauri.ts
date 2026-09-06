/**
 * Tauri の `invoke` / `listen` を使った実データ実装。
 *
 * コマンド呼び出しの命名規則（重要）:
 * - コマンド名（`invoke` の第一引数）は Rust の `#[tauri::command]` 関数名と同じ
 *   snake_case のまま使う（例: `'get_thread'`）。
 * - 一方、**引数名は Tauri v2 の既定でキャメルケースに変換される**。つまり Rust の
 *   `fn get_thread(state, thread_key: String)` は JS からは
 *   `invoke('get_thread', { threadKey })` で呼ぶ。`get_digest(start_of_day, date_key)` は
 *   `invoke('get_digest', { startOfDay, dateKey })`、`mark(ids, action)` は `{ ids, action }`。
 * - ただし**構造体のフィールド名は serde の属性に従う**。`ThreadFilter` と
 *   `NewAccountInput` は `#[serde(rename_all = "snake_case")]` が付いているので、
 *   これらを渡すときのオブジェクトのキー自体は snake_case のまま（`filter` や
 *   `input` という引数名だけがキャメルケース、中身は変換しない）。
 *
 * コマンドが返すエラーは `{ code, message }`（`AppError`）。`invoke` は reject するので
 * catch した値がこの形になる。
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { Account } from '../types';
import type {
  DigestDto,
  DraftDto,
  LastSync,
  MessageDto,
  NewAccountInput,
  NewDraftInput,
  ProjectGroup,
  SyncProgressEvent,
  TestConnectionResult,
  ThreadDetailDto,
  ThreadFilter,
  ViewCountsDto,
} from '../types.api';
import { defaultAiDraftBody, mockAiDraftBodies } from '../mock/drafts';
import type {
  Digest,
  ProjectGroupView,
  SyncState,
  ThreadDetail,
  ThreadListItem,
  ViewItemView,
} from '../types.ui';
import type { ThreadSummary } from '../types';
import type { MarkAction, MeowboxApi, ThreadQuery } from './contract';
import {
  digestToView,
  projectGroupToView,
  threadDetailToView,
  threadSummaryToListItem,
  viewCountsToViews,
} from './map';

/** Rust 側の `SYNC_PROGRESS_EVENT` と一致させること */
const SYNC_PROGRESS_EVENT = 'sync://progress';

export async function listAccounts(): Promise<Account[]> {
  return invoke<Account[]>('list_accounts');
}

export async function addAccount(input: NewAccountInput): Promise<Account> {
  return invoke<Account>('add_account', { input });
}

export async function setAccountPassword(id: number, password: string): Promise<void> {
  await invoke('set_account_password', { id, password });
}

export async function testConnection(
  input: NewAccountInput,
  password: string,
): Promise<TestConnectionResult> {
  return invoke<TestConnectionResult>('test_connection', { input, password });
}

export async function deleteAccount(id: number): Promise<void> {
  await invoke('delete_account', { id });
}

export async function listProjects(): Promise<ProjectGroupView[]> {
  const [groups, syncMap] = await Promise.all([
    invoke<ProjectGroup[]>('list_projects'),
    listSyncStatus(),
  ]);
  return groups.map((g) => projectGroupToView(g, projectSyncState(g, syncMap)));
}

/**
 * 案件グループの同期状態。グループ内のどれかのアカウントがエラーなら 'error'、
 * 1 つも同期していなければ 'warn'、それ以外は 'ok'。
 * 「同期中」は判定材料が無いので出さない。
 */
function projectSyncState(group: ProjectGroup, syncMap: Record<number, LastSync>): SyncState {
  const entries = group.accounts.map((a) => syncMap[a.id]).filter((s): s is LastSync => !!s);
  if (entries.some((s) => s.error)) return 'error';
  if (entries.length === 0) return 'warn';
  return 'ok';
}

export async function listViews(): Promise<ViewItemView[]> {
  const counts = await invoke<ViewCountsDto>('view_counts');
  return viewCountsToViews(counts);
}

export async function listThreads(query: ThreadQuery = {}): Promise<ThreadListItem[]> {
  // 'tasks' / 'drafts' はスレッド一覧ではないので空配列を返す
  // TODO(P3-b): タスク一覧・下書き一覧の専用データ取得を用意する
  if (query.view === 'tasks' || query.view === 'drafts') {
    return [];
  }

  const filter: ThreadFilter = {
    project_tag: query.project_tag ?? undefined,
    account_id: query.account_id ?? undefined,
    unread_only: query.view === 'unread',
    flagged_only: query.view === 'flagged',
    limit: query.limit,
    offset: query.offset,
  };

  const threads = await invoke<ThreadSummary[]>('list_threads', { filter });
  return threads.map((t) => threadSummaryToListItem(t));
}

export async function getThread(threadKey: string): Promise<ThreadDetail | null> {
  try {
    const dto = await invoke<ThreadDetailDto>('get_thread', { threadKey });
    return threadDetailToView(dto);
  } catch (e) {
    if (isNotFound(e)) return null;
    throw e;
  }
}

export async function getMessage(id: number): Promise<MessageDto | null> {
  try {
    return await invoke<MessageDto>('get_message', { id });
  } catch (e) {
    if (isNotFound(e)) return null;
    throw e;
  }
}

function isNotFound(e: unknown): boolean {
  return (
    typeof e === 'object' &&
    e !== null &&
    'code' in e &&
    (e as { code: unknown }).code === 'not_found'
  );
}

export async function extractAttachment(id: number): Promise<string> {
  return invoke<string>('extract_attachment', { id });
}

/** 添付を展開して OS の既定アプリで開く。パスの検証は Rust 側で行う */
export async function openAttachment(id: number): Promise<void> {
  await invoke('open_attachment', { id });
}

/** ローカルの「今日」の 0:00 を作る */
function localStartOfDay(now: Date): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate());
}

/** `YYYY-MM-DD`。ローカルの年月日から組む（`toISOString` の先頭 10 文字は UTC なのでズレる） */
function localDateKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

export async function getDigest(startOfDay?: Date): Promise<Digest> {
  const now = new Date();
  const start = startOfDay ?? localStartOfDay(now);
  const dto = await invoke<DigestDto>('get_digest', {
    startOfDay: start.toISOString(),
    dateKey: localDateKey(start),
  });
  return digestToView(dto, now);
}

export async function mark(ids: number[], action: MarkAction): Promise<number> {
  return invoke<number>('mark', { ids, action });
}

export async function syncAccount(id: number): Promise<void> {
  await invoke('sync_account', { id });
}

export async function listSyncStatus(): Promise<Record<number, LastSync>> {
  return invoke<Record<number, LastSync>>('sync_status');
}

export async function onSyncProgress(cb: (e: SyncProgressEvent) => void): Promise<() => void> {
  const unlisten = await listen<SyncProgressEvent>(SYNC_PROGRESS_EVENT, (e) => cb(e.payload));
  return unlisten;
}

/** `now` からの経過分を計算する。テストで固定できるよう純粋関数にしてある */
function minutesAgo(finishedAt: string, now: Date): number {
  const diffMs = now.getTime() - new Date(finishedAt).getTime();
  return Math.max(0, Math.floor(diffMs / 60000));
}

/**
 * `listAccounts` / `listSyncStatus` から `{ state, label }` を組み立てる純粋関数。
 * `invoke` を呼ばないので `now` を固定してテストできる。
 */
export function computeSyncStatus(
  accounts: Account[],
  syncMap: Record<number, LastSync>,
  now: Date,
): { state: SyncState; label: string } {
  if (accounts.length === 0) {
    return { state: 'warn', label: 'アカウント未登録' };
  }

  const entries = accounts.map((a) => syncMap[a.id]).filter((s): s is LastSync => !!s);
  const errored = entries.find((s) => s.error);
  if (errored) {
    return { state: 'error', label: `エラー: ${errored.error}` };
  }
  if (entries.length === 0) {
    return { state: 'warn', label: '未同期' };
  }

  const latest = entries.reduce((a, b) =>
    new Date(a.finished_at) > new Date(b.finished_at) ? a : b,
  );
  return { state: 'ok', label: `${minutesAgo(latest.finished_at, now)}分前に同期` };
}

export async function getSyncStatus(): Promise<{ state: SyncState; label: string }> {
  const [accounts, syncMap] = await Promise.all([listAccounts(), listSyncStatus()]);
  return computeSyncStatus(accounts, syncMap, new Date());
}

/**
 * 返信下書きを保存する。**送信はしない**（MVP の安全境界）。
 * MCP 側にも送信ツールは出さず、送信は UI の「確認して送信」だけが行う。
 */
export async function createDraft(input: NewDraftInput): Promise<DraftDto> {
  return invoke<DraftDto>('create_draft', { input });
}

export async function listDrafts(): Promise<DraftDto[]> {
  return invoke<DraftDto[]>('list_drafts');
}

/**
 * Claude に返信下書きを書かせる。P3 で MCP / Claude API に繋ぐ。
 * それまでは既存のまま（中身は据え置き）。
 */
export async function generateAiDraft(threadKey: string): Promise<string> {
  return mockAiDraftBodies[threadKey] ?? defaultAiDraftBody;
}

/** 「確認して送信」。人間が承認したときだけ呼ばれる */
export async function sendDraft(input: { thread_key: string; body: string }): Promise<void> {
  // TODO(P3): invoke('send_draft', input) で mailsync の SMTP に流す
  void input;
}

export const tauriApi = {
  listAccounts,
  addAccount,
  setAccountPassword,
  testConnection,
  deleteAccount,
  listProjects,
  listViews,
  listThreads,
  getThread,
  getMessage,
  extractAttachment,
  openAttachment,
  getDigest,
  mark,
  syncAccount,
  listSyncStatus,
  onSyncProgress,
  getSyncStatus,
  createDraft,
  listDrafts,
  generateAiDraft,
  sendDraft,
} satisfies MeowboxApi;
