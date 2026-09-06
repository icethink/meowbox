import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { Digest } from '../../types.ui';

const emptyDigest: Digest = {
  date_label: '9/6 (日)',
  summary: [],
  groups: [],
};

vi.mock('../../api', () => ({
  getDigest: vi.fn().mockResolvedValue(emptyDigest),
}));

const { DigestPanel } = await import('./DigestPanel');

describe('DigestPanel', () => {
  it('要約・タスクとも空のダイジェストで両方の空状態文言を出す', async () => {
    render(<DigestPanel />);

    expect(await screen.findByText('今日のまとめはまだありません')).toBeInTheDocument();
    expect(screen.getByText('Claude が MCP 経由で書き込むとここに出ます')).toBeInTheDocument();
    expect(screen.getByText('抽出されたタスクはありません')).toBeInTheDocument();
  });
});
