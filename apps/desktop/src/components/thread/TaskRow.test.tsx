import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Task } from '../../types';
import { TaskRow } from './TaskRow';

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

describe('TaskRow', () => {
  it('確度が高いタスクは確定として期日つきで出す', () => {
    render(<TaskRow task={makeTask()} />);

    expect(screen.getByText('見積書を送る')).toBeInTheDocument();
    expect(screen.getByText('確定')).toBeInTheDocument();
    expect(screen.getByText('9/5 (金)')).toBeInTheDocument();
    // 確定行に「候補」の確度表示や確定/却下ボタンは出さない
    expect(screen.queryByRole('button', { name: '却下' })).not.toBeInTheDocument();
  });

  it('確度が低いタスクは候補として確度と確定/却下を出す', async () => {
    const onConfirm = vi.fn();
    const onDismiss = vi.fn();
    render(
      <TaskRow
        task={makeTask({ title: '追加要件のヒアリング日程調整', due: null, confidence: 0.62 })}
        onConfirm={onConfirm}
        onDismiss={onDismiss}
      />,
    );

    expect(screen.getByText('候補 · 確度 62%')).toBeInTheDocument();
    expect(screen.queryByText('確定')).not.toBe(null);

    await userEvent.click(screen.getByRole('button', { name: '確定' }));
    expect(onConfirm).toHaveBeenCalledOnce();

    await userEvent.click(screen.getByRole('button', { name: '却下' }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it('候補のチェックボックスは点線で、まだ完了にはできない', () => {
    render(<TaskRow task={makeTask({ confidence: 0.4 })} />);

    const box = screen.getByRole('checkbox');
    expect(box.className).toContain('border-dashed');
    expect(box).toHaveAttribute('aria-checked', 'false');
  });
});
