import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AccountWizard } from './AccountWizard';
import { EmptyState } from './EmptyState';

/** App.tsx の組み合わせを模した小さなハーネス */
function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <EmptyState onAddAccount={() => setOpen(true)} />
      <AccountWizard open={open} onClose={() => setOpen(false)} onAdded={() => {}} />
    </>
  );
}

async function fillValidServerStep(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText('表示名'), 'テスト用アカウント');
  await user.type(screen.getByLabelText('メールアドレス'), 'you@example.com');
  await user.type(screen.getByLabelText('パスワード'), 'hunter2');
}

describe('EmptyState + AccountWizard', () => {
  it('「アカウントを追加」を押すとウィザードが開く', async () => {
    const user = userEvent.setup();
    render(<Harness />);

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '種別を選ぶ' })).toBeInTheDocument();
  });

  it('ステップ 1 で Gmail と Microsoft 365 は選べない', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));

    expect(screen.getByRole('button', { name: /Gmail/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Microsoft 365/ })).toBeDisabled();
  });

  it('接続テストをしていない間は「次へ」を押せない', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    await fillValidServerStep(user);

    expect(screen.getByRole('button', { name: '次へ' })).toBeDisabled();
  });

  it('接続テスト成功後に「次へ」が押せるようになり、入力を変えるとまた押せなくなる', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));
    await fillValidServerStep(user);

    await user.click(screen.getByRole('button', { name: '接続テスト' }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '次へ' })).toBeEnabled();
    });
    expect(screen.getByText(/接続できました/)).toBeInTheDocument();

    await user.type(screen.getByLabelText('表示名'), '追記');

    expect(screen.getByRole('button', { name: '次へ' })).toBeDisabled();
    expect(screen.queryByText(/接続できました/)).not.toBeInTheDocument();
  });

  it('メールアドレスを入れるとホストが仮埋めされる', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    await user.type(screen.getByLabelText('メールアドレス'), 'you@example.com');

    expect(screen.getByLabelText('ホスト')).toHaveValue('imap.example.com');
  });

  it('不正なポートを入れるとエラー文言が出る', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    await user.type(screen.getByLabelText('ポート'), 'abc');

    expect(await screen.findByText('数字で入力してください')).toBeInTheDocument();
  });

  it('パスワードを入れてキャンセルで閉じ、再度開くとパスワード欄が空になっている', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    await user.type(screen.getByLabelText('パスワード'), 'hunter2');
    expect(screen.getByLabelText('パスワード')).toHaveValue('hunter2');

    // モーダル外側のクリックでキャンセル相当（Modal の onClose）
    await user.click(screen.getByRole('dialog').parentElement as HTMLElement);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    expect(screen.getByLabelText('パスワード')).toHaveValue('');
  });

  it('パスワードを入れて Esc で閉じ、再度開くとパスワード欄が空になっている', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    await user.type(screen.getByLabelText('パスワード'), 'hunter2');
    expect(screen.getByLabelText('パスワード')).toHaveValue('hunter2');

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    expect(screen.getByLabelText('パスワード')).toHaveValue('');
  });

  it('接続テストから完了まで進んだあと、開き直してもパスワード欄は空になっている', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));
    await fillValidServerStep(user);
    await user.click(screen.getByRole('button', { name: '接続テスト' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '次へ' })).toBeEnabled();
    });
    await user.click(screen.getByRole('button', { name: '次へ' }));
    await user.click(screen.getByRole('button', { name: '完了' }));

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: 'アカウントを追加' }));
    await user.click(screen.getByRole('button', { name: '次へ' }));

    expect(screen.getByLabelText('パスワード')).toHaveValue('');
  });
});
