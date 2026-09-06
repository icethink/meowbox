/**
 * 歯車から開く設定モーダル。アカウント一覧・再同期・削除だけを扱う。
 * 削除は `window.confirm` を使わず、行の中身をインライン確認に差し替える。
 */
import { useEffect, useRef, useState } from 'react';
import { deleteAccount, listAccounts, mcpIntegration, syncAccount } from '../../api';
import { useAppStore } from '../../store/app';
import type { Account } from '../../types';
import type { McpIntegration } from '../../types.api';
import { Badge } from '../ui/Badge';
import { Checkbox } from '../ui/Checkbox';
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

/** クリップボードにコピーするボタン。成功したら 2 秒だけラベルを変える */
function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      timeoutRef.current = setTimeout(() => setCopied(false), 2000);
    } catch {
      // クリップボードが使えない環境でも画面を壊さない
    }
  }

  return (
    <button
      type="button"
      onClick={() => void handleCopy()}
      className="shrink-0 rounded-token bg-accent px-[10px] py-[4px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover"
    >
      {copied ? 'コピーしました' : 'コピー'}
    </button>
  );
}

/** 一般設定の節。60 秒ポーリングの on/off は人間の操作なので accent 系の色を使う */
function GeneralSection() {
  const autoRefresh = useAppStore((s) => s.autoRefresh);
  const setAutoRefresh = useAppStore((s) => s.setAutoRefresh);

  return (
    <div className="mt-[16px] border-t border-line pt-[16px]">
      <h3 className="mb-[6px] text-base font-bold text-primary">全般</h3>
      <div className="flex items-center gap-[10px]">
        <Checkbox checked={autoRefresh} onChange={setAutoRefresh} label="60 秒ごとに自動更新" />
        <span className="text-base text-secondary">60 秒ごとに自動更新</span>
      </div>
      <p className="mt-[4px] text-12 text-muted">
        既定はオフ。オンにすると 60 秒ごとに一覧を読み直します。
      </p>
    </div>
  );
}

/** Claude Desktop / Claude Code への登録に使うコピペ用の節。人間が行う設定操作なので accent 系の色を使う */
function ClaudeIntegrationSection({ mcp }: { mcp: McpIntegration | null }) {
  return (
    <div className="mt-[16px] border-t border-line pt-[16px]">
      <h3 className="mb-[6px] text-base font-bold text-primary">Claude 連携</h3>
      <p className="mb-[10px] text-12 text-secondary">
        Claude Desktop / Claude Code から Meowbox のメールを検索・要約できるようにします。
        設定ファイルは自動で書き換えません。下の内容をコピーして貼ってください。
      </p>

      {mcp && !mcp.server_exists && (
        <p className="mb-[10px] text-12 text-warn">
          meowbox-mcp が見つかりません（開発中は `pnpm mcp:build` を実行してください）
        </p>
      )}

      {mcp && (
        <div className="flex flex-col gap-[12px]">
          <div>
            <div className="mb-[4px] flex items-center justify-between gap-[10px]">
              <span className="text-12 font-bold text-secondary">Claude Desktop</span>
              <CopyButton text={mcp.desktop_config_json} />
            </div>
            <pre className="overflow-x-auto rounded-token border border-line bg-subtle p-[10px] font-mono text-11 text-body">
              {mcp.desktop_config_json}
            </pre>
          </div>

          <div>
            <div className="mb-[4px] flex items-center justify-between gap-[10px]">
              <span className="text-12 font-bold text-secondary">Claude Code / Cowork</span>
              <CopyButton text={mcp.claude_code_command} />
            </div>
            <pre className="overflow-x-auto rounded-token border border-line bg-subtle p-[10px] font-mono text-11 text-body">
              {mcp.claude_code_command}
            </pre>
          </div>
        </div>
      )}
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
  const [mcp, setMcp] = useState<McpIntegration | null>(null);

  useEffect(() => {
    if (!open) {
      // モーダルを閉じたら確認中の状態はリセットする
      setConfirmingId(null);
      return;
    }
    void listAccounts().then(setAccounts);
    void mcpIntegration().then(setMcp);
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

      <GeneralSection />
      <ClaudeIntegrationSection mcp={mcp} />
    </Modal>
  );
}
