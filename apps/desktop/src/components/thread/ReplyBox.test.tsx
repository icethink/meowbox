import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useAppStore } from '../../store/app';
import { ReplyBox } from './ReplyBox';

function setup() {
  return render(<ReplyBox threadKey="th-estimate" placeholder="山田さんへ返信…" />);
}

describe('ReplyBox', () => {
  beforeEach(() => {
    useAppStore.setState({ replyBody: '', replyIsUneditedAiDraft: false, toast: null });
  });

  it('AI の下書きを一度も編集せずに送ろうとしたら確認ダイアログを出す', async () => {
    setup();

    await userEvent.click(screen.getByRole('button', { name: /AI で下書き/ }));
    const input = await screen.findByRole<HTMLTextAreaElement>('textbox', { name: '返信本文' });
    expect(input.value).toContain('山田様');

    await userEvent.click(screen.getByRole('button', { name: '確認して送信' }));

    expect(
      await screen.findByRole('heading', { name: 'AI の下書きをそのまま送信しますか？' }),
    ).toBeInTheDocument();
    // ダイアログを出しただけで送信はしていない
    expect(useAppStore.getState().toast).toBeNull();
  });

  it('人間が本文を編集していれば確認ダイアログは出さずに送る', async () => {
    setup();

    await userEvent.click(screen.getByRole('button', { name: /AI で下書き/ }));
    await userEvent.type(screen.getByRole('textbox', { name: '返信本文' }), '追記します。');

    await userEvent.click(screen.getByRole('button', { name: '確認して送信' }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(useAppStore.getState().toast).toBe('送信しました');
    expect(useAppStore.getState().replyBody).toBe('');
  });

  it('確認ダイアログで「このまま送信」を押せば送信できる', async () => {
    setup();

    await userEvent.click(screen.getByRole('button', { name: /AI で下書き/ }));
    await userEvent.click(screen.getByRole('button', { name: '確認して送信' }));
    await userEvent.click(await screen.findByRole('button', { name: 'このまま送信' }));

    expect(useAppStore.getState().toast).toBe('送信しました');
  });

  it('本文が空なら何も起きない', async () => {
    setup();

    await userEvent.click(screen.getByRole('button', { name: '確認して送信' }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(useAppStore.getState().toast).toBeNull();
  });
});
