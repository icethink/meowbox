import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ThreadMessageView } from '../../types.ui';

const openAttachment = vi.fn().mockResolvedValue(undefined);

vi.mock('../../api', () => ({
  openAttachment: (id: number) => openAttachment(id),
}));

const { MessageItem } = await import('./MessageItem');

const message: ThreadMessageView = {
  id: 1,
  from: { name: '山田 太郎', email: 'yamada@example.com' },
  initial: 'Y',
  time_label: '9/1 17:20',
  body: [{ text: '本文です。' }],
  quoted_lines: 0,
  quoted_text: '',
  attachments: [{ id: 42, filename: '見積書.pdf', size_label: '18KB' }],
  is_latest: true,
};

describe('MessageItem', () => {
  it('添付チップをクリックすると openAttachment が添付 id で呼ばれる', async () => {
    render(<MessageItem message={message} />);

    await userEvent.click(screen.getByRole('button', { name: /見積書\.pdf/ }));

    expect(openAttachment).toHaveBeenCalledWith(42);
  });
});
