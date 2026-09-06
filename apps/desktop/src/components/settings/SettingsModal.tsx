/**
 * 歯車から開く設定モーダル。アカウント一覧・再同期・削除だけを扱う。
 * 削除は `window.confirm` を使わず、行の中身をインライン確認に差し替える。
 */
import { useEffect, useState } from 'react';
import { deleteAccount, listAccounts, syncAccount } from '../../api';
import type { Account } from '../../types';
import { Badge } from '../ui/Badge';
import { Modal } from '../ui/Modal';

function errorMessage(e: unknown): string {
  if (
    e &&
    typeof e === 'object' &&
    'message' in e &&
    typeof (e as { message: unknown }).message === 'string'
  ) {
    return (e as { message: string }).message;
  }
  return '不明なエラーが発生しました';
}

function AccountRow({
  account,
  syncing,
  syncError,
  confirmingDelete,
  onSync,
  onDeleteClick,
  onConfirmDelete,
  onCancelDelete,
}: {
  account: Account;
  syncing: boolean;
  syncError: string | undefined;
  confirmingDelete: boolean;
  onSync: () => void;
  onDeleteClick: () => void;
  onConfirmDelete: () => void;
  onCancelDelete: () => void;
}) {
  if (confirmingDelete) {
    return (
      <div className="flex items-center justify-between gap-[10px] rounded-token border border-danger-line bg-danger-bg px-[10px] py-[8px]">
        <span className="flex-1 text-base text-danger">
          {account.email} を削除します。保存済みのメールも消えます。
        </span>
        <div className="flex shrink-0 gap-[6px]">
          <button
            type="button"
            onClick={onConfirmDelete}
            className="rounded-token border border-danger-line px-[10px] py-[4px] text-12 font-bold text-danger transition-colors hover:bg-danger-bg"
          >
            削除する
          </button>
          <button
            type="button"
            onClick={onCancelDelete}
            className="rounded-token border border-line-soft px-[10px] py-[4px] text-12 text-muted transition-colors hover:bg-selected"
          >
            やめる
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between gap-[10px] rounded-token border border-line px-[10px] py-[8px]">
      <div className="flex flex-1 flex-col gap-[2px]">
        <span className="text-base text-primary">{account.name}</span>
        <span className="font-mono text-11 text-muted">{account.email}</span>
        {account.project_tag ? (
          <Badge tone="neutral">{account.project_tag}</Badge>
        ) : (
          <span className="text-11 text-faint">未分類</span>
        )}
        {syncError && <span className="text-11 text-danger">{syncError}</span>}
      </div>
      <div className="flex shrink-0 gap-[6px]">
        <button
          type="button"
          onClick={onSync}
          disabled={syncing}
          className="rounded-token border border-line-soft px-[10px] py-[4px] text-12 text-secondary transition-colors hover:bg-selected disabled:opacity-50"
        >
          {syncing ? '再同期中…' : '再同期'}
        </button>
        <button
          type="button"
          onClick={onDeleteClick}
          className="rounded-token border border-line-soft px-[10px] py-[4px] text-12 text-danger transition-colors hover:bg-danger-bg"
        >
          削除
        </button>
      </div>
    </div>
  );
}

export function SettingsModal({
  open,
  onClose,
  onChanged,
}: {
  open: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [syncingId, setSyncingId] = useState<number | null>(null);
  const [syncErrors, setSyncErrors] = useState<Record<number, string>>({});
  const [confirmingId, setConfirmingId] = useState<number | null>(null);

  useEffect(() => {
    if (!open) {
      // モーダルを閉じたら確認中の状態はリセットする
      setConfirmingId(null);
      return;
    }
    void listAccounts().then(setAccounts);
  }, [open]);

  async function reloadAccounts() {
    setAccounts(await listAccounts());
  }

  async function handleSync(id: number) {
    setSyncErrors((m) => {
      const next = { ...m };
      delete next[id];
      return next;
    });
    setSyncingId(id);
    try {
      await syncAccount(id);
      onChanged();
    } catch (e) {
      setSyncErrors((m) => ({ ...m, [id]: errorMessage(e) }));
    } finally {
      setSyncingId(null);
    }
  }

  async function handleConfirmDelete(id: number) {
    await deleteAccount(id);
    setConfirmingId(null);
    await reloadAccounts();
    onChanged();
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      labelledBy="settings-modal-title"
      className="w-[560px] p-[18px]"
    >
      <div className="mb-[12px] flex items-center justify-between">
        <h2 id="settings-modal-title" className="text-md font-bold text-primary">
          設定
        </h2>
        <button
          type="button"
          onClick={onClose}
          className="rounded-token border border-line-soft px-[10px] py-[4px] text-12 text-muted transition-colors hover:bg-selected"
        >
          閉じる
        </button>
      </div>

      {accounts.length === 0 ? (
        <p className="text-base text-faint">アカウントがありません</p>
      ) : (
        <div className="flex flex-col gap-[8px]">
          {accounts.map((a) => (
            <AccountRow
              key={a.id}
              account={a}
              syncing={syncingId === a.id}
              syncError={syncErrors[a.id]}
              confirmingDelete={confirmingId === a.id}
              onSync={() => void handleSync(a.id)}
              onDeleteClick={() => setConfirmingId(a.id)}
              onConfirmDelete={() => void handleConfirmDelete(a.id)}
              onCancelDelete={() => setConfirmingId(null)}
            />
          ))}
        </div>
      )}
    </Modal>
  );
}
