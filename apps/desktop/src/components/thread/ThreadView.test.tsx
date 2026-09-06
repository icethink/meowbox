import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ThreadDetail } from '../../types.ui';
import { ThreadView } from './ThreadView';

function makeThread(overrides: Partial<ThreadDetail> = {}): ThreadDetail {
  return {
    thread_key: 'th-1',
    subject: '件名',
    project_tag: '案件A',
    counterpart: { name: '山田 太郎', email: 'yamada@client-a.example' },
    message_count: 1,
    summary: null,
    tasks: [],
    messages: [
      {
        id: 1,
        from: { name: '山田 太郎', email: 'yamada@client-a.example' },
        initial: '山',
        time_label: '9/1 17:20',
        body: [{ text: '本文です。' }],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [],
        is_latest: true,
        is_read: false,
      },
    ],
    reply_placeholder: '山田さんへ返信…',
    ...overrides,
  };
}

describe('ThreadView', () => {
  it('summary が null のときは要約の空状態を出す', () => {
    render(<ThreadView thread={makeThread({ summary: null })} onArchive={() => {}} />);

    expect(screen.getByText('要約はまだありません')).toBeInTheDocument();
  });

  it('summary があるときは要約本文を出す', () => {
    render(
      <ThreadView
        thread={makeThread({
          summary: {
            target: 'thread:th-1',
            body: [{ text: '見積の返信待ちです。' }],
            generated_label: '10:02',
            generated_at: '2025-09-02T01:02:00Z',
          },
        })}
        onArchive={() => {}}
      />,
    );

    expect(screen.getByText('見積の返信待ちです。')).toBeInTheDocument();
  });
});
