/**
 * モック実装。`VITE_MEOWBOX_MOCK=1` のとき（vitest と、Tauri 無しでの見た目確認用）に使う。
 * P2 まで `index.ts` に直接書かれていたものをそのままここへ移した。
 */

import type { Account } from '../types';
import type {
  DraftDto,
  LastSync,
  MessageDto,
  NewAccountInput,
  NewDraftInput,
  TestConnectionResult,
} from '../types.api';
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
import type { MarkAction, MeowboxApi, ThreadQuery } from './contract';

/** 送信はまだ実装していない（下書きの保存まで。モックでも嘘の成功を出さない） */
export const sendAvailable = false;

export async function listAccounts(): Promise<Account[]> {
  return mockAccounts;
}

/** 渡された入力から Account を組み立てて返す。UI が動く程度の素朴な実装 */
export async function addAccount(input: NewAccountInput): Promise<Account> {
  return {
    id: Date.now(),
    name: input.name,
    kind: input.kind,
    email: input.email,
    project_tag: input.project_tag,
    settings: {
      host: input.host,
      port: input.port,
      username: input.username,
      starttls: input.starttls,
    },
    created_at: new Date().toISOString(),
  };
}

export async function setAccountPassword(_id: number, _password: string): Promise<void> {
  // モックでは何もしない
}

export async function testConnection(
  _input: NewAccountInput,
  _password: string,
): Promise<TestConnectionResult> {
  return { folders: ['INBOX'] };
}

export async function deleteAccount(_id: number): Promise<void> {
  // モックでは何もしない
}

export async function listProjects(): Promise<ProjectGroupView[]> {
  return mockProjects;
}

export async function listViews(): Promise<ViewItemView[]> {
  return mockViews;
}

export async function getSyncStatus(): Promise<{
  state: typeof mockSyncStatus.state;
  label: string;
}> {
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

/** `mockThreadDetails` から適当に組み立てる。見つからなければ null */
export async function getMessage(id: number): Promise<MessageDto | null> {
  for (const detail of Object.values(mockThreadDetails)) {
    const msg = detail.messages.find((m) => m.id === id);
    if (!msg) continue;
    return {
      id: msg.id,
      account_id: 0,
      thread_key: detail.thread_key,
      from: msg.from,
      to: [],
      cc: [],
      subject: detail.subject,
      date: new Date().toISOString(),
      body_text: msg.body.map((span) => span.text).join(''),
      quoted_text: msg.quoted_text,
      has_attachments: msg.attachments.length > 0,
      is_read: true,
      is_flagged: false,
      attachments: msg.attachments.map((a) => ({
        id: a.id,
        filename: a.filename,
        mime: 'application/octet-stream',
        size: 0,
      })),
    };
  }
  return null;
}

export async function extractAttachment(_id: number): Promise<string> {
  return '/mock/attachment.txt';
}

export async function openAttachment(_id: number): Promise<void> {
  // モックでは何もしない
}

export async function getDigest(_startOfDay?: Date): Promise<Digest> {
  return mockDigest;
}

export async function mark(_ids: number[], _action: MarkAction): Promise<number> {
  // モックでは何もしない
  return 0;
}

export async function syncAccount(_id: number): Promise<void> {
  // モックでは何もしない
}

export async function listSyncStatus(): Promise<Record<number, LastSync>> {
  return {};
}

export async function onSyncProgress(): Promise<() => void> {
  // モックでは何も購読しない
  return () => {};
}

/** 呼び出しごとに増える id。モックの下書きに一意な id を振るのに使う */
let mockDraftSeq = 0;

/**
 * 返信下書きを保存する。**送信はしない**（MVP の安全境界）。
 * MCP 側にも送信ツールは出さず、送信は UI の「確認して送信」だけが行う。
 */
export async function createDraft(input: NewDraftInput): Promise<DraftDto> {
  mockDraftSeq += 1;
  return {
    id: mockDraftSeq,
    account_id: 0,
    in_reply_to: input.in_reply_to,
    to: [],
    subject: '',
    body: input.body,
    status: 'draft',
    created_at: new Date().toISOString(),
  };
}

export async function listDrafts(): Promise<DraftDto[]> {
  return [];
}

/** Claude に返信下書きを書かせる。P3 で MCP / Claude API に繋ぐ */
export async function generateAiDraft(threadKey: string): Promise<string> {
  return mockAiDraftBodies[threadKey] ?? defaultAiDraftBody;
}

/** 「確認して送信」。送信はまだ実装していない（モックでも嘘の成功を出さない） */
// TODO(P3-b): SMTP を繋いだら sendAvailable を true にする
export async function sendDraft(_input: { thread_key: string; body: string }): Promise<void> {
  throw new Error('送信はまだ実装されていません');
}

export const mockApi = {
  sendAvailable,
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
