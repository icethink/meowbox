import { useCallback, useEffect, useMemo, useState } from 'react';
import { AppShell } from './components/layout/AppShell';
import { Sidebar } from './components/sidebar/Sidebar';
import { ThreadList } from './components/list/ThreadList';
import { ThreadRow } from './components/list/ThreadRow';
import { ThreadView } from './components/thread/ThreadView';
import { DigestPanel } from './components/panel/DigestPanel';
import { CommandPalette } from './components/CommandPalette';
import { AccountWizard } from './components/onboarding/AccountWizard';
import { EmptyState } from './components/onboarding/EmptyState';
import { Toast } from './components/ui/Toast';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import {
  getThread,
  listAccounts,
  listThreads,
  listViews,
  mark,
  onSyncProgress,
  syncAccount,
} from './api';
import type { Account } from './types';
import type { ThreadDetail, ThreadListItem, ViewItemView } from './types.ui';
import { useAppStore } from './store/app';

export default function App() {
  const {
    rightPanelOpen,
    selectedThreadKey,
    selectThread,
    activeProjectTag,
    activeView,
    accountWizardOpen,
    setAccountWizardOpen,
  } = useAppStore();

  const [accounts, setAccounts] = useState<Account[] | null>(null);
  const [threads, setThreads] = useState<ThreadListItem[]>([]);
  const [views, setViews] = useState<ViewItemView[]>([]);
  const [selected, setSelected] = useState<ThreadDetail | null>(null);

  async function refreshAccounts() {
    const list = await listAccounts();
    setAccounts(list);
  }

  const refreshThreads = useCallback(async () => {
    const list = await listThreads({ project_tag: activeProjectTag, view: activeView });
    setThreads(list);
  }, [activeProjectTag, activeView]);

  const refreshViews = useCallback(async () => {
    const list = await listViews();
    setViews(list);
  }, []);

  useEffect(() => {
    void refreshAccounts();
  }, []);

  useEffect(() => {
    void refreshViews();
  }, [refreshViews]);

  useEffect(() => {
    void refreshThreads();
  }, [refreshThreads]);

  // 同期が完了したら一覧・ビュー件数を読み直す
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await onSyncProgress((e) => {
        if (e.done) {
          void refreshThreads();
          void refreshViews();
        }
      });
    })();
    return () => unlisten?.();
  }, [refreshThreads, refreshViews]);

  function handleAdded(accountId: number) {
    void syncAccount(accountId);
    void refreshAccounts();
  }

  // 選択中のスレッドの詳細を読み直す。開いたスレッドの未読メッセージは既読にする
  useEffect(() => {
    if (!selectedThreadKey) {
      setSelected(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      const detail = await getThread(selectedThreadKey);
      if (cancelled) return;
      setSelected(detail);

      const unreadIds = detail?.messages.filter((m) => !m.is_read).map((m) => m.id) ?? [];
      if (unreadIds.length === 0) return;
      try {
        await mark(unreadIds, 'read');
        if (cancelled) return;
        setSelected((prev) =>
          prev && prev.thread_key === detail?.thread_key
            ? { ...prev, messages: prev.messages.map((m) => ({ ...m, is_read: true })) }
            : prev,
        );
        await refreshThreads();
        await refreshViews();
      } catch (err) {
        console.error(err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedThreadKey, refreshThreads, refreshViews]);

  const threadKeys = useMemo(() => threads.map((t) => t.thread_key), [threads]);
  useKeyboardShortcuts({ threadKeys, selectedThreadKey });

  const view = views.find((v) => v.key === activeView);
  const listTitle = view?.label ?? 'すべて';
  // 案件で絞っているときは表示中の件数、そうでなければビューの総数を出す
  const listCount = activeProjectTag ? threads.length : (view?.count ?? threads.length);

  /** アーカイブしたら、その位置にあった次のスレッドへ選択を送る */
  async function handleArchive(key: string) {
    const index = threadKeys.indexOf(key);
    const after = threadKeys[index + 1] ?? threadKeys[index - 1];
    const detail = selected && selected.thread_key === key ? selected : await getThread(key);
    const ids = detail?.messages.map((m) => m.id) ?? [];
    if (ids.length > 0) {
      await mark(ids, 'archive');
    }
    await refreshThreads();
    if (after) selectThread(after);
  }

  // 読み込み中は何も出さない（起動直後のちらつき防止）
  if (accounts === null) return null;

  return (
    <>
      {accounts.length === 0 ? (
        <EmptyState onAddAccount={() => setAccountWizardOpen(true)} />
      ) : (
        <AppShell
          sidebar={<Sidebar />}
          list={
            <ThreadList title={listTitle} count={listCount}>
              {threads.map((t) => (
                <ThreadRow
                  key={t.thread_key}
                  thread={t}
                  selected={t.thread_key === selectedThreadKey}
                  onSelect={() => selectThread(t.thread_key)}
                  onArchive={() => void handleArchive(t.thread_key)}
                />
              ))}
            </ThreadList>
          }
          thread={
            <ThreadView
              thread={selected}
              onArchive={() => selectedThreadKey && void handleArchive(selectedThreadKey)}
            />
          }
          panel={rightPanelOpen ? <DigestPanel /> : null}
        />
      )}
      <AccountWizard
        open={accountWizardOpen}
        onClose={() => setAccountWizardOpen(false)}
        onAdded={handleAdded}
      />
      <CommandPalette />
      <Toast />
    </>
  );
}
