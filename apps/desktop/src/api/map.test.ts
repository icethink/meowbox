import { describe, expect, it } from 'vitest';

import type { ThreadSummary } from '../types';
import type { DigestDto, ThreadDetailDto, ViewCountsDto } from '../types.api';
import {
  digestToView,
  formatFileSize,
  threadDetailToView,
  threadSummaryToListItem,
  viewCountsToViews,
} from './map';

const NOW = new Date('2025-09-02T10:00:00+09:00');

function makeThreadSummary(overrides: Partial<ThreadSummary> = {}): ThreadSummary {
  return {
    thread_key: 'th-1',
    account_id: 1,
    project_tag: '案件A',
    latest_message_id: 100,
    subject: '件名',
    from: { name: '山田 太郎', email: 'yamada@client-a.example' },
    snippet: '本文の抜粋…',
    last_date: '2025-09-02T00:30:00Z',
    message_count: 2,
    unread_count: 1,
    has_attachments: false,
    is_flagged: false,
    ...overrides,
  };
}

describe('threadSummaryToListItem', () => {
  it('does not fabricate an ai summary', () => {
    const item = threadSummaryToListItem(makeThreadSummary(), NOW);
    expect(item.ai_snippet).toBeNull();
    expect(item.snippet).toBe('本文の抜粋…');
  });

  it('falls back project_tag and from_name when missing', () => {
    const item = threadSummaryToListItem(
      makeThreadSummary({ project_tag: null, from: { name: null, email: 'a@mail.example' } }),
      NOW,
    );
    expect(item.project_tag).toBe('未分類');
    expect(item.from_name).toBe('a@mail.example');
  });

  it('formats the relative time label with a fixed now', () => {
    const item = threadSummaryToListItem(
      makeThreadSummary({ last_date: '2025-09-02T00:30:00Z' }),
      NOW,
    );
    expect(item.time_label).toBe('9:30');
  });

  it('derives is_unread from unread_count', () => {
    expect(threadSummaryToListItem(makeThreadSummary({ unread_count: 0 }), NOW).is_unread).toBe(
      false,
    );
    expect(threadSummaryToListItem(makeThreadSummary({ unread_count: 3 }), NOW).is_unread).toBe(
      true,
    );
  });
});

function makeThreadDetail(overrides: Partial<ThreadDetailDto> = {}): ThreadDetailDto {
  return {
    thread_key: 'th-1',
    subject: '件名',
    project_tag: '案件A',
    summary: null,
    tasks: [],
    messages: [
      {
        id: 1,
        account_id: 1,
        thread_key: 'th-1',
        from: { name: '山田 太郎', email: 'yamada@client-a.example' },
        to: [],
        cc: [],
        subject: '件名',
        date: '2025-09-01T08:20:00Z',
        body_text: '本文1',
        quoted_text: '',
        has_attachments: false,
        is_read: true,
        is_flagged: false,
        attachments: [],
      },
      {
        id: 2,
        account_id: 1,
        thread_key: 'th-1',
        from: { name: '山田 太郎', email: 'yamada@client-a.example' },
        to: [],
        cc: [],
        subject: '件名',
        date: '2025-09-02T00:41:00Z',
        body_text: '本文2',
        quoted_text: '> 引用1行目\n> 引用2行目\n> 引用3行目',
        has_attachments: false,
        is_read: false,
        is_flagged: false,
        attachments: [],
      },
    ],
    ...overrides,
  };
}

describe('threadDetailToView', () => {
  it('keeps summary null when the DTO has no summary', () => {
    const view = threadDetailToView(makeThreadDetail(), NOW);
    expect(view.summary).toBeNull();
  });

  it('builds a summary from the DTO when present', () => {
    const view = threadDetailToView(
      makeThreadDetail({
        summary: {
          target: 'thread:th-1',
          model: 'claude',
          summary: '要約テキスト',
          created_at: '2025-09-02T01:00:00Z',
        },
      }),
      NOW,
    );
    expect(view.summary).not.toBeNull();
    expect(view.summary?.body).toEqual([{ text: '要約テキスト' }]);
  });

  it('counts quoted lines and reports 0 when there is no quote', () => {
    const view = threadDetailToView(makeThreadDetail(), NOW);
    expect(view.messages[0]!.quoted_lines).toBe(0);
    expect(view.messages[1]!.quoted_lines).toBe(3);
  });

  it('marks only the last message as latest', () => {
    const view = threadDetailToView(makeThreadDetail(), NOW);
    expect(view.messages[0]!.is_latest).toBe(false);
    expect(view.messages[1]!.is_latest).toBe(true);
  });
});

describe('formatFileSize', () => {
  it('formats bytes', () => {
    expect(formatFileSize(512)).toBe('512 B');
  });

  it('formats kilobytes as an integer', () => {
    expect(formatFileSize(18 * 1024)).toBe('18KB');
  });

  it('formats megabytes with one decimal place', () => {
    expect(formatFileSize(1.2 * 1024 * 1024)).toBe('1.2MB');
  });
});

function makeDigest(overrides: Partial<DigestDto> = {}): DigestDto {
  return {
    summary: null,
    groups: [
      {
        project_tag: '案件A',
        tasks: [
          {
            id: 1,
            account_id: 1,
            source_message_id: null,
            title: '今日期限のタスク',
            due: '2025-09-02T00:00:00Z',
            status: 'open',
            confidence: 0.9,
            created_by: 'ai',
            created_at: '2025-09-01T00:00:00Z',
          },
          {
            id: 2,
            account_id: 1,
            source_message_id: null,
            title: '来週期限のタスク',
            due: '2025-09-08T00:00:00Z',
            status: 'open',
            confidence: 0.9,
            created_by: 'ai',
            created_at: '2025-09-01T00:00:00Z',
          },
          {
            id: 3,
            account_id: 1,
            source_message_id: null,
            title: '来月期限のタスク',
            due: '2025-10-08T00:00:00Z',
            status: 'open',
            confidence: 0.9,
            created_by: 'ai',
            created_at: '2025-09-01T00:00:00Z',
          },
          {
            id: 4,
            account_id: 1,
            source_message_id: null,
            title: '確度の低いタスク',
            due: null,
            status: 'open',
            confidence: 0.5,
            created_by: 'ai',
            created_at: '2025-09-01T00:00:00Z',
          },
        ],
      },
    ],
    ...overrides,
  };
}

describe('digestToView', () => {
  it('returns an empty summary array when the DTO has no summary', () => {
    const digest = digestToView(makeDigest(), NOW);
    expect(digest.summary).toEqual([]);
  });

  it('classifies due_tone into danger / accent / neutral', () => {
    const digest = digestToView(makeDigest(), NOW);
    const items = digest.groups[0]!.items;
    expect(items[0]!.due_tone).toBe('danger'); // 今日
    expect(items[1]!.due_tone).toBe('accent'); // 7日以内
    expect(items[2]!.due_tone).toBe('neutral'); // それより先
    expect(items[3]!.due_tone).toBe('neutral'); // 期限なし
  });

  it('marks low-confidence tasks as candidates', () => {
    const digest = digestToView(makeDigest(), NOW);
    const items = digest.groups[0]!.items;
    expect(items[0]!.is_candidate).toBe(false);
    expect(items[3]!.is_candidate).toBe(true);
  });
});

describe('viewCountsToViews', () => {
  it('returns the 6 views with the same labels and tones as before', () => {
    const counts: ViewCountsDto = { all: 24, unread: 9, flagged: 2, tasks: 6, drafts: 1 };
    const views = viewCountsToViews(counts);
    expect(views).toEqual([
      { key: 'all', label: 'すべて', count: 24, tone: null },
      { key: 'unread', label: '未読', count: 9, tone: null },
      { key: 'action', label: '要対応', count: 0, tone: 'accent' },
      { key: 'flagged', label: 'フラグ', count: 2, tone: null },
      { key: 'tasks', label: 'タスク', count: 6, tone: 'ai' },
      { key: 'drafts', label: '下書き', count: 1, tone: null },
    ]);
  });
});
