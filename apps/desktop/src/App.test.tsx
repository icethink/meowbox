import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

/**
 * VITE_MEOWBOX_MOCK=1 なので `./api` は `mock.ts` の実装（アカウントあり・
 * スレッドありのモックデータ）を返す。ここでは「一覧が listThreads の結果
 * から描画される」ことだけを確認する。
 */
describe('App', () => {
  it('アカウントがある状態で一覧に listThreads の結果が描画される', async () => {
    render(<App />);

    expect(await screen.findByText('山田 太郎')).toBeInTheDocument();
    expect(screen.getByText(/見積の件/)).toBeInTheDocument();
  });
});
