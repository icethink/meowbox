import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import App from './App';
import { useAppStore } from './store/app';

/**
 * VITE_MEOWBOX_MOCK=1 なので `./api` は基本的に `mock.ts` の実装
 * （アカウントあり・スレッドありのモックデータ）を使う。`mark` だけ
 * `vi.fn` で包み、呼び出しを検証できるようにする。
 */
vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return {
    ...actual,
    mark: vi.fn(actual.mark),
  };
});

const { mark } = await import('./api');

describe('App', () => {
  it('アカウントがある状態で一覧に listThreads の結果が描画される', async () => {
    render(<App />);

    expect(await screen.findByText('山田 太郎')).toBeInTheDocument();
    expect(screen.getByText(/見積の件/)).toBeInTheDocument();
  });

  describe('スレッドを開いたときの既読化', () => {
    beforeEach(() => {
      vi.mocked(mark).mockClear();
      useAppStore.setState({ selectedThreadKey: null });
    });

    it('未読メッセージがあるスレッドを開くと、その id で mark(id, "read") が呼ばれる', async () => {
      render(<App />);
      await screen.findByText('山田 太郎');

      act(() => {
        useAppStore.getState().selectThread('th-estimate');
      });

      await waitFor(() => {
        expect(mark).toHaveBeenCalledWith([1002], 'read');
      });
    });

    it('未読メッセージが無いスレッドを開いても mark は呼ばれない', async () => {
      render(<App />);
      await screen.findByText('山田 太郎');

      act(() => {
        useAppStore.getState().selectThread('th-monthly-meeting');
      });

      expect(await screen.findByRole('heading', { name: '9月定例のご案内' })).toBeInTheDocument();
      expect(mark).not.toHaveBeenCalled();
    });
  });
});
