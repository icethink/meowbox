import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsModal } from './SettingsModal';
import { deleteAccount, listAccounts, syncAccount } from '../../api';
import type { Account } from '../../types';

vi.mock('../../api', () => ({
  listAccounts: vi.fn(),
  deleteAccount: vi.fn(),
  syncAccount: vi.fn(),
}));

const accounts: Account[] = [
  {
    id: 1,
    name: '自社',
    kind: 'imap',
    email: 'me@my-company.example',
    project_tag: '自社',
    settings: {},
    created_at: '2025-06-01T00:00:00Z',
  },
  {
    id: 2,
    name: '未分類アカウント',
    kind: 'imap',
    email: 'nobody@unclassified.example',
    project_tag: null,
    settings: {},
    created_at: '2025-06-01T00:00:00Z',
  },
];

describe('SettingsModal', () => {
  beforeEach(() => {
    vi.mocked(listAccounts).mockReset().mockResolvedValue(accounts);
    vi.mocked(deleteAccount).mockReset().mockResolvedValue(undefined);
    vi.mocked(syncAccount).mockReset().mockResolvedValue(undefined);
  });

  it('アカウント一覧が表示される', async () => {
    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);

    expect(await screen.findByText('me@my-company.example')).toBeInTheDocument();
    expect(screen.getByText('nobody@unclassified.example')).toBeInTheDocument();
    expect(screen.getByText('未分類')).toBeInTheDocument();
  });

  it('「削除」を押すと確認行に切り替わり、まだ deleteAccount は呼ばれない', async () => {
    const user = userEvent.setup();
    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);
    await screen.findByText('me@my-company.example');

    const deleteButtons = screen.getAllByRole('button', { name: '削除' });
    await user.click(deleteButtons[0]!);

    expect(await screen.findByText(/me@my-company.example を削除します/)).toBeInTheDocument();
    expect(deleteAccount).not.toHaveBeenCalled();
  });

  it('「削除する」を押して初めて deleteAccount が呼ばれる', async () => {
    const user = userEvent.setup();
    const onChanged = vi.fn();
    render(<SettingsModal open onClose={() => {}} onChanged={onChanged} />);
    await screen.findByText('me@my-company.example');

    await user.click(screen.getAllByRole('button', { name: '削除' })[0]!);
    await user.click(await screen.findByRole('button', { name: '削除する' }));

    await waitFor(() => {
      expect(deleteAccount).toHaveBeenCalledWith(1);
    });
    expect(onChanged).toHaveBeenCalled();
  });

  it('「やめる」で確認行が消える', async () => {
    const user = userEvent.setup();
    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);
    await screen.findByText('me@my-company.example');

    await user.click(screen.getAllByRole('button', { name: '削除' })[0]!);
    await screen.findByText(/me@my-company.example を削除します/);

    await user.click(screen.getByRole('button', { name: 'やめる' }));

    expect(screen.queryByText(/を削除します/)).not.toBeInTheDocument();
    expect(deleteAccount).not.toHaveBeenCalled();
  });

  it('「再同期」で syncAccount がその id で呼ばれる', async () => {
    const user = userEvent.setup();
    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);
    await screen.findByText('me@my-company.example');

    await user.click(screen.getAllByRole('button', { name: '再同期' })[0]!);

    await waitFor(() => {
      expect(syncAccount).toHaveBeenCalledWith(1);
    });
  });
});
