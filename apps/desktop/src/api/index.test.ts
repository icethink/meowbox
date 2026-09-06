import { describe, expect, it } from 'vitest';

import type { Account } from '../types';
import type { LastSync } from '../types.api';
import { listAccounts } from './index';
import { computeSyncStatus } from './tauri';

const NOW = new Date('2025-09-02T10:00:00+09:00');

function makeAccount(id: number): Account {
  return {
    id,
    name: `account-${id}`,
    kind: 'imap',
    email: `account-${id}@mail.example`,
    project_tag: null,
    settings: {},
    created_at: '2025-06-01T00:00:00Z',
  };
}

describe('index (vitest はモックを使う)', () => {
  it('uses the mock implementation under VITE_MEOWBOX_MOCK=1', async () => {
    const accounts = await listAccounts();
    expect(accounts.length).toBeGreaterThan(0);
  });
});

describe('computeSyncStatus', () => {
  it('reports no accounts registered when there are none', () => {
    const status = computeSyncStatus([], {}, NOW);
    expect(status).toEqual({ state: 'warn', label: 'アカウント未登録' });
  });

  it('reports not synced yet when no account has a LastSync entry', () => {
    const status = computeSyncStatus([makeAccount(1)], {}, NOW);
    expect(status).toEqual({ state: 'warn', label: '未同期' });
  });

  it('reports the error message when any account failed', () => {
    const syncMap: Record<number, LastSync> = {
      1: {
        finished_at: '2025-09-02T00:50:00Z',
        inserted: 0,
        errors: 0,
        error: '接続できませんでした',
      },
    };
    const status = computeSyncStatus([makeAccount(1)], syncMap, NOW);
    expect(status).toEqual({ state: 'error', label: 'エラー: 接続できませんでした' });
  });

  it('reports minutes since the latest successful sync', () => {
    const syncMap: Record<number, LastSync> = {
      1: { finished_at: '2025-09-02T00:50:00Z', inserted: 3, errors: 0, error: null },
      2: { finished_at: '2025-09-02T00:55:00Z', inserted: 1, errors: 0, error: null },
    };
    const status = computeSyncStatus([makeAccount(1), makeAccount(2)], syncMap, NOW);
    // NOW は 10:00 JST = 01:00 UTC。最新は 00:55 UTC なので 5 分前。
    expect(status).toEqual({ state: 'ok', label: '5分前に同期' });
  });
});
