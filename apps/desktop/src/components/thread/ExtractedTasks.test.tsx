import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Task } from '../../types';
import { useAppStore } from '../../store/app';
import { ExtractedTasks } from './ExtractedTasks';

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 1,
    account_id: 2,
    source_message_id: 10,
    title: '見積書を送る',
    due: '2025-09-05T09:00:00Z',
    status: 'open',
    confidence: 0.94,
    created_by: 'ai',
    created_at: '2025-09-02T01:02:00Z',
    ...overrides,
  };
}

describe('ExtractedTasks', () => {
  beforeEach(() => {
    useAppStore.setState({ taskDecisions: {} });
  });

  it('タスクが 0 件のときは空状態を出す', () => {
    render(<ExtractedTasks tasks={[]} />);

    expect(screen.getByText('タスクはまだ抽出されていません')).toBeInTheDocument();
  });

  it('すべて dismiss されて 0 件になった場合は空状態を出さない', async () => {
    const task = makeTask({ confidence: 0.4 });
    render(<ExtractedTasks tasks={[task]} />);

    await userEvent.click(screen.getByRole('button', { name: '却下' }));

    expect(screen.queryByText('タスクはまだ抽出されていません')).not.toBeInTheDocument();
    expect(screen.queryByText('見積書を送る')).not.toBeInTheDocument();
  });
});
