/**
 * Rust から来る DTO（`types.api.ts`）を UI 型（`types.ui.ts`）に変換する。
 * コンポーネントはこの変換後の型だけを受け取り続ける。
 */

import { formatDueDate, formatMessageTime, formatRelativeDate } from '../lib/relativeDate';
import type { Address, Task, ThreadSummary } from '../types';
import type { DigestDto, ProjectGroup, ThreadDetailDto, ViewCountsDto } from '../types.api';
import type {
  Digest,
  DigestItem,
  ProjectGroupView,
  RichSpan,
  SyncState,
  ThreadDetail,
  ThreadListItem,
  ThreadMessageView,
  ViewItemView,
} from '../types.ui';

const WEEKDAY_KANJI = ['日', '月', '火', '水', '木', '金', '土'];

/** スレッド一覧の 1 行に変換する。Claude の要約はまだ無いので `ai_snippet` は必ず null */
export function threadSummaryToListItem(t: ThreadSummary, now?: Date): ThreadListItem {
  return {
    thread_key: t.thread_key,
    account_id: t.account_id,
    project_tag: t.project_tag ?? '未分類',
    from_name: t.from.name ?? t.from.email,
    subject: t.subject,
    ai_snippet: null,
    snippet: t.snippet,
    time_label: formatRelativeDate(t.last_date, now),
    date: t.last_date,
    is_unread: t.unread_count > 0,
    urgency: null,
    has_attachments: t.has_attachments,
    task_count: 0,
    is_muted: false,
  };
}

/** アバターに出す 1 文字。`name` の先頭、無ければ `email` の先頭を大文字化する */
function initialOf(address: Address): string {
  const source = address.name && address.name.length > 0 ? address.name : address.email;
  const first = Array.from(source)[0] ?? '';
  return first.toUpperCase();
}

/** "18KB" / "1.2MB" のような表示用文字列を組み立てる */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)}KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}

function messageDtoToView(
  m: ThreadDetailDto['messages'][number],
  isLatest: boolean,
  now?: Date,
): ThreadMessageView {
  return {
    id: m.id,
    from: m.from,
    initial: initialOf(m.from),
    time_label: formatMessageTime(m.date, now),
    body: [{ text: m.body_text }],
    quoted_lines: m.quoted_text === '' ? 0 : m.quoted_text.split('\n').length,
    quoted_text: m.quoted_text,
    attachments: m.attachments.map((a) => ({
      id: a.id,
      filename: a.filename,
      size_label: formatFileSize(a.size),
    })),
    is_latest: isLatest,
    is_read: m.is_read,
  };
}

/** スレッド詳細を UI 型に変換する。Claude の要約が無ければ `summary` は null のまま */
export function threadDetailToView(d: ThreadDetailDto, now?: Date): ThreadDetail {
  // TODO(P3-b): 自アカウントのアドレスを除いて相手を選ぶ
  const counterpart: Address = d.messages[0]?.from ?? { name: null, email: '' };
  const counterpartName = counterpart.name ?? counterpart.email;

  const summary = d.summary
    ? {
        target: d.summary.target,
        body: [{ text: d.summary.summary }] as RichSpan[],
        generated_label: formatMessageTime(d.summary.created_at, now),
        generated_at: d.summary.created_at,
        model: d.summary.model,
      }
    : null;

  const lastIndex = d.messages.length - 1;

  return {
    thread_key: d.thread_key,
    subject: d.subject,
    project_tag: d.project_tag ?? '未分類',
    counterpart,
    message_count: d.messages.length,
    summary,
    tasks: d.tasks,
    messages: d.messages.map((m, i) => messageDtoToView(m, i === lastIndex, now)),
    reply_placeholder: `${counterpartName}さんへ返信…`,
  };
}

/** `date` の `now` からの日数差（`date` が翌日なら 1、前日なら -1）。ローカルのカレンダー日で比較する */
function calendarDayDiff(date: Date, now: Date): number {
  const a = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const b = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  return Math.round((a - b) / (24 * 60 * 60 * 1000));
}

function dueTone(due: string | null, now: Date): DigestItem['due_tone'] {
  if (!due) return 'neutral';
  const d = new Date(due);
  if (Number.isNaN(d.getTime())) return 'neutral';
  const diff = calendarDayDiff(d, now);
  if (diff <= 0) return 'danger';
  if (diff <= 7) return 'accent';
  return 'neutral';
}

function taskToDigestItem(t: Task, now: Date): DigestItem {
  return {
    id: t.id,
    title: t.title,
    due_label: t.due ? formatDueDate(t.due, now) : null,
    due_tone: dueTone(t.due, now),
    is_candidate: t.confidence < 0.7,
  };
}

/** "M/D (曜)" を組み立てる */
function formatDateLabel(now: Date): string {
  return `${now.getMonth() + 1}/${now.getDate()} (${WEEKDAY_KANJI[now.getDay()]})`;
}

/** ダイジェスト DTO を UI 型に変換する。Claude の要約が無ければ `summary` は空配列 */
export function digestToView(d: DigestDto, now: Date): Digest {
  return {
    date_label: formatDateLabel(now),
    summary: d.summary ? [{ text: d.summary.summary }] : [],
    groups: d.groups.map((g) => ({
      project_tag: g.project_tag ?? '未分類',
      items: g.tasks.map((t) => taskToDigestItem(t, now)),
    })),
  };
}

/** サイドバーの案件グループを UI 型に変換する。`has_action` は判定材料がまだ無いので常に false */
export function projectGroupToView(g: ProjectGroup, sync: SyncState): ProjectGroupView {
  return {
    tag: g.tag ?? '未分類',
    accounts: g.accounts,
    unread: g.unread,
    has_action: false,
    sync,
  };
}

/** サイドバーの「ビュー」セクション。ラベル・tone は既存 `mockViews` と同じ。`action` は判定材料が無いので 0 */
export function viewCountsToViews(c: ViewCountsDto): ViewItemView[] {
  return [
    { key: 'all', label: 'すべて', count: c.all, tone: null },
    { key: 'unread', label: '未読', count: c.unread, tone: null },
    { key: 'action', label: '要対応', count: 0, tone: 'accent' },
    { key: 'flagged', label: 'フラグ', count: c.flagged, tone: null },
    { key: 'tasks', label: 'タスク', count: c.tasks, tone: 'ai' },
    { key: 'drafts', label: '下書き', count: c.drafts, tone: null },
  ];
}
