import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { AiSummary } from '../../types.ui';
import { AiSummaryCard } from './AiSummaryCard';

function makeSummary(): AiSummary {
  return {
    target: 'thread',
    body: [{ text: '見積の返信待ちです。' }],
    generated_label: '10:02',
    generated_at: '2025-09-02T01:02:00Z',
  };
}

describe('AiSummaryCard', () => {
  it('summary が null のときは空状態を出し、再生成ボタンは出さない', () => {
    render(<AiSummaryCard summary={null} />);

    expect(screen.getByText('要約はまだありません')).toBeInTheDocument();
    expect(screen.getByText('Claude が MCP 経由で書き込むとここに出ます')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /再生成/ })).not.toBeInTheDocument();
  });

  it('summary があるときは本文を出す（既存の動作）', () => {
    render(<AiSummaryCard summary={makeSummary()} />);

    expect(screen.getByText('見積の返信待ちです。')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /再生成/ })).toBeInTheDocument();
  });
});
