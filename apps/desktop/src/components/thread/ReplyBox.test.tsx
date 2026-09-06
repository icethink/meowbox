import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useAppStore } from '../../store/app';

/**
 * VITE_MEOWBOX_MOCK=1 なので実体は `mock.ts`。`sendAvailable` は false・
 * `aiDraftAvailable` は true のまま、呼び出しだけ検証できるよう vi.fn で包む。
 */
vi.mock('../../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api')>();
  return {
    ...actual,
    createDraft: vi.fn(actual.createDraft),
    generateAiDraft: vi.fn(actual.generateAiDraft),
    sendDraft: vi.fn(actual.sendDraft),
  };
});

const { createDraft, sendDraft } = await import('../../api');
const { ReplyBox } = await import('./ReplyBox');

function setup(inReplyTo: number | null = 1) {
  return render(
    <ReplyBox threadKey="th-estimate" placeholder="山田さんへ返信…" inReplyTo={inReplyTo} />,
  );
}

describe('ReplyBox', () => {
  beforeEach(() => {
    useAppStore.setState({ replyBody: '', replyIsUneditedAiDraft: false, toast: null });
    vi.mocked(createDraft).mockClear();
    vi.mocked(sendDraft).mockClear();
  });

  it('送信は未対応なので「確認して送信」は disabled で、押しても sendDraft は呼ばれず「送信しました」は出ない', async () => {
    setup();
    await userEvent.type(screen.getByRole('textbox', { name: '返信本文' }), 'こんにちは');

    const sendButton = screen.getByRole('button', { name: '確認して送信' });
    expect(sendButton).toBeDisabled();

    await userEvent.click(sendButton);

    expect(sendDraft).not.toHaveBeenCalled();
    expect(useAppStore.getState().toast).not.toBe('送信しました');
    expect(screen.getByText('送信は未対応です（下書きの保存まで）')).toBeInTheDocument();
  });

  it('「下書きを保存」を押すと本文と in_reply_to つきで createDraft が呼ばれる', async () => {
    setup(42);
    await userEvent.type(screen.getByRole('textbox', { name: '返信本文' }), '追記します。');

    await userEvent.click(screen.getByRole('button', { name: '下書きを保存' }));

    expect(createDraft).toHaveBeenCalledWith({ in_reply_to: 42, body: '追記します。' });
    await vi.waitFor(() => {
      expect(useAppStore.getState().toast).toBe('下書きを保存しました');
    });
  });

  it('inReplyTo が null のとき「下書きを保存」は disabled', async () => {
    setup(null);
    await userEvent.type(screen.getByRole('textbox', { name: '返信本文' }), 'こんにちは');

    expect(screen.getByRole('button', { name: '下書きを保存' })).toBeDisabled();
  });

  it('本文が空白のみのときは「下書きを保存」を押しても createDraft は呼ばれない', async () => {
    setup(1);
    await userEvent.type(screen.getByRole('textbox', { name: '返信本文' }), '   ');

    expect(screen.getByRole('button', { name: '下書きを保存' })).toBeDisabled();
    expect(createDraft).not.toHaveBeenCalled();
  });

  it('AI で下書きを挿入すると本文に反映される（送信は未対応のまま）', async () => {
    setup();

    await userEvent.click(screen.getByRole('button', { name: /AI で下書き/ }));

    const input = await screen.findByRole<HTMLTextAreaElement>('textbox', { name: '返信本文' });
    expect(input.value).toContain('山田様');
    expect(screen.getByRole('button', { name: '確認して送信' })).toBeDisabled();
  });
});

describe('ReplyBox (aiDraftAvailable = false)', () => {
  it('Claude の下書きが使えないときは「AI で下書き」が disabled', async () => {
    vi.resetModules();
    vi.doMock('../../api', () => ({
      createDraft: vi.fn(),
      generateAiDraft: vi.fn(),
      sendDraft: vi.fn(),
      sendAvailable: false,
      aiDraftAvailable: false,
    }));

    const { ReplyBox: ReplyBoxAiOff } = await import('./ReplyBox');
    render(<ReplyBoxAiOff threadKey="th-estimate" placeholder="山田さんへ返信…" inReplyTo={1} />);

    const button = screen.getByRole('button', { name: /AI で下書き/ });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Claude の下書きは P1 で対応');
  });
});
