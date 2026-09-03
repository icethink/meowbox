/**
 * データアクセス層。UI からはこの関数群だけを呼ぶ。
 *
 * P2 の今はモックを同期的に返しているが、シグネチャは Promise にしてある。
 * P3 で中身を `invoke('list_threads', …)` に差し替えれば UI 側は変更不要。
 */

import type { Account, Draft } from '../types';
import type {
  Digest,
  ProjectGroupView,
  ThreadDetail,
  ThreadListItem,
  ViewItemView,
} from '../types.ui';
import { mockAccounts, mockProjects, mockSyncStatus, mockViews } from '../mock/accounts';
import { mockThreadDetails, mockThreads } from '../mock/threads';
import { mockDigest } from '../mock/digest';
import { defaultAiDraftBody, mockAiDraftBodies } from '../mock/drafts';

export interface ThreadQuery {
  /** 案件タグで絞る。null は全案件 */
  project_tag?: string | null;
  /** サイドバーのビュー */
  view?: string;
}

export async function listAccounts(): Promise<Account[]> {
  return mockAccounts;
}

export async function listProjects(): Promise<ProjectGroupView[]> {
  return mockProjects;
}

export async function listViews(): Promise<ViewItemView[]> {
  return mockViews;
}

export async function getSyncStatus(): Promise<typeof mockSyncStatus> {
  return mockSyncStatus;
}

export async function listThreads(query: ThreadQuery = {}): Promise<ThreadListItem[]> {
  const { project_tag = null } = query;
  if (!project_tag) return mockThreads;
  return mockThreads.filter((t) => t.project_tag === project_tag);
}

export async function getThread(threadKey: string): Promise<ThreadDetail | null> {
  return mockThreadDetails[threadKey] ?? null;
}

export async function getDigest(): Promise<Digest> {
  return mockDigest;
}

/**
 * 返信下書きを保存する。**送信はしない**（MVP の安全境界）。
 * MCP 側にも送信ツールは出さず、送信は UI の「確認して送信」だけが行う。
 */
export async function createDraft(input: {
  thread_key: string;
  body: string;
}): Promise<Pick<Draft, 'id' | 'body' | 'status'>> {
  return { id: Date.now(), body: input.body, status: 'draft' };
}

/** Claude に返信下書きを書かせる。P3 で MCP / Claude API に繋ぐ */
export async function generateAiDraft(threadKey: string): Promise<string> {
  return mockAiDraftBodies[threadKey] ?? defaultAiDraftBody;
}

/** 「確認して送信」。人間が承認したときだけ呼ばれる */
export async function sendDraft(input: { thread_key: string; body: string }): Promise<void> {
  // TODO(P3): invoke('send_draft', input) で mailsync の SMTP に流す
  void input;
}
