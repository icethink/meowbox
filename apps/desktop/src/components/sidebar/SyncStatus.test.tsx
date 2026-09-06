import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SyncStatus } from './SyncStatus';

describe('SyncStatus', () => {
  it('歯車を押すと onOpenSettings が呼ばれる', async () => {
    const user = userEvent.setup();
    const onOpenSettings = vi.fn();
    render(<SyncStatus state="ok" label="2分前に同期" onOpenSettings={onOpenSettings} />);

    await user.click(screen.getByRole('button', { name: '設定' }));

    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });
});
