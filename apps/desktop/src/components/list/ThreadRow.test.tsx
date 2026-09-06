import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ThreadListItem } from '../../types.ui';
import { ThreadRow } from './ThreadRow';

const base: ThreadListItem = {
  thread_key: 'th-1',
  account_id: 1,
  project_tag: '案件A',
  from_name: '山田 太郎',
  subject: '見積の件',
  ai_snippet: null,
  snippet: '本文の抜粋がここに入ります。',
  time_label: '10:00',
  date: '2025-09-02T01:00:00Z',
  is_unread: true,
  urgency: null,
  has_attachments: false,
  task_count: 0,
  is_muted: false,
};

function setup(thread: ThreadListItem) {
  return render(
    <ThreadRow thread={thread} selected={false} onSelect={vi.fn()} onArchive={vi.fn()} />,
  );
}

describe('ThreadRow', () => {
  it('ai_snippet が無ければ本文の抜粋を text-muted で出す', () => {
    setup(base);

    const snippet = screen.getByText(base.snippet);
    expect(snippet).toHaveClass('text-muted');
    expect(screen.queryByText(/一覧画面で/)).not.toBeInTheDocument();
  });

  it('ai_snippet があればそちらを出し、本文の抜粋は出さない', () => {
    setup({ ...base, ai_snippet: 'Claude による要約です' });

    const aiSnippet = screen.getByText('Claude による要約です');
    expect(aiSnippet).toHaveClass('text-ai-muted');
    expect(screen.queryByText(base.snippet)).not.toBeInTheDocument();
  });

  it('ai_snippet も snippet も無ければ 3 行目は出さない', () => {
    setup({ ...base, snippet: '' });

    expect(screen.queryByText(base.snippet)).not.toBeInTheDocument();
  });
});
