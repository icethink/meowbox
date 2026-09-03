import type { Account } from '../types';
import type { ProjectGroupView, ViewItemView } from '../types.ui';

/**
 * デザイン（docs/design/main-dark.png）と同じサンプル。
 * アドレスは RFC 2606 の `.example` を使い、実在しうるドメインは置かない。
 */
export const mockAccounts: Account[] = [
  {
    id: 1,
    name: '自社',
    kind: 'imap',
    email: 'me@my-company.example',
    project_tag: '自社',
    settings: { host: 'imap.my-company.example', port: 993 },
    created_at: '2025-06-01T00:00:00Z',
  },
  {
    id: 2,
    name: '案件A (メイン)',
    kind: 'imap',
    email: 'a-project@client-a.example',
    project_tag: '案件A',
    settings: { host: 'imap.client-a.example', port: 993 },
    created_at: '2025-07-10T00:00:00Z',
  },
  {
    id: 3,
    name: '案件A (共有)',
    kind: 'gmail',
    email: 'a-project@gmail.example',
    project_tag: '案件A',
    settings: {},
    created_at: '2025-07-10T00:00:00Z',
  },
  {
    id: 4,
    name: '案件B',
    kind: 'm365',
    email: 'b@client-b.example',
    project_tag: '案件B',
    settings: {},
    created_at: '2025-08-01T00:00:00Z',
  },
];

/** サイドバーの「案件」セクション。1 案件に複数アドレスが束ねられる */
export const mockProjects: ProjectGroupView[] = [
  {
    tag: '自社',
    accounts: [{ id: 1, email: 'me@my-company.example' }],
    unread: 3,
    has_action: true,
    sync: 'ok',
  },
  {
    tag: '案件A',
    accounts: [
      { id: 2, email: 'a-project@client-a.example' },
      { id: 3, email: 'a-project@gmail.example' },
    ],
    unread: 5,
    has_action: true,
    sync: 'ok',
  },
  {
    tag: '案件B',
    accounts: [{ id: 4, email: 'b@client-b.example' }],
    unread: 1,
    has_action: false,
    sync: 'warn',
  },
];

/** サイドバーの「ビュー」セクション */
export const mockViews: ViewItemView[] = [
  { key: 'all', label: 'すべて', count: 24, tone: null },
  { key: 'unread', label: '未読', count: 9, tone: null },
  { key: 'action', label: '要対応', count: 4, tone: 'accent' },
  { key: 'flagged', label: 'フラグ', count: 2, tone: null },
  { key: 'tasks', label: 'タスク', count: 6, tone: 'ai' },
  { key: 'drafts', label: '下書き', count: 1, tone: null },
];

/** フッタの同期状態 */
export const mockSyncStatus = { state: 'ok' as const, label: '2分前に同期' };
