import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsModal } from './SettingsModal';
import { deleteAccount, listAccounts, mcpIntegration, syncAccount } from '../../api';
import type { Account } from '../../types';
import type { McpIntegration } from '../../types.api';

vi.mock('../../api', () => ({
  listAccounts: vi.fn(),
  deleteAccount: vi.fn(),
  syncAccount: vi.fn(),
  mcpIntegration: vi.fn(),
}));

const mcp: McpIntegration = {
  server_path: 'C:\\Program Files\\Meowbox\\meowbox-mcp.exe',
  server_exists: true,
  desktop_config_json: JSON.stringify(
    {
      mcpServers: {
        meowbox: { command: 'C:\\Program Files\\Meowbox\\meowbox-mcp.exe', args: [] },
      },
    },
    null,
    2,
  ),
  claude_code_command: 'claude mcp add meowbox -- "C:\\Program Files\\Meowbox\\meowbox-mcp.exe"',
};

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
    vi.mocked(mcpIntegration).mockReset().mockResolvedValue(mcp);
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

  it('「Claude 連携」の節が表示され、claude mcp add コマンドが出る', async () => {
    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);

    expect(await screen.findByText('Claude 連携')).toBeInTheDocument();
    expect(screen.getByText(/claude mcp add meowbox/)).toBeInTheDocument();
  });

  it('「コピー」を押すと navigator.clipboard.writeText が呼ばれる', async () => {
    // userEvent.setup() は独自の clipboard スタブを navigator.clipboard に付け替えるので、
    // それより後に上書きする（先にやると setup() 側の getter に上書きされてしまう）
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);
    await screen.findByText('Claude 連携');

    const copyButtons = screen.getAllByRole('button', { name: 'コピー' });
    await user.click(copyButtons[0]!);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(mcp.desktop_config_json);
    });

    await user.click(copyButtons[1]!);
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(mcp.claude_code_command);
    });
  });

  it('server_exists が false のとき警告文が出る', async () => {
    vi.mocked(mcpIntegration).mockResolvedValue({ ...mcp, server_exists: false });

    render(<SettingsModal open onClose={() => {}} onChanged={() => {}} />);

    expect(await screen.findByText(/meowbox-mcp が見つかりません/)).toBeInTheDocument();
  });
});
