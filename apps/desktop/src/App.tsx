import { useEffect, useMemo, useState } from 'react';
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
import { mockThreadDetails, mockThreads } from './mock/threads';
import { mockViews } from './mock/accounts';
import { listAccounts, syncAccount } from './api';
import type { Account } from './types';
import { useAppStore } from './store/app';

export default function App() {
  const {
    rightPanelOpen,
    selectedThreadKey,
    selectThread,
    archivedKeys,
    archiveThread,
    readKeys,
    activeProjectTag,
    activeView,
    accountWizardOpen,
    setAccountWizardOpen,
  } = useAppStore();

  const [accounts, setAccounts] = useState<Account[] | null>(null);

  async function refreshAccounts() {
    const list = await listAccounts();
    setAccounts(list);
  }

  useEffect(() => {
    void refreshAccounts();
  }, []);

  function handleAdded(accountId: number) {
    void syncAccount(accountId);
    void refreshAccounts();
  }

  const threads = useMemo(
    () =>
      mockThreads
        .filter((t) => !archivedKeys.includes(t.thread_key))
        .filter((t) => !activeProjectTag || t.project_tag === activeProjectTag)
        // 開いたスレッドは既読にする
        .map((t) => (readKeys.includes(t.thread_key) ? { ...t, is_unread: false } : t)),
    [archivedKeys, activeProjectTag, readKeys],
  );

  const threadKeys = useMemo(() => threads.map((t) => t.thread_key), [threads]);
  useKeyboardShortcuts({ threadKeys, selectedThreadKey });

  const selected = selectedThreadKey ? (mockThreadDetails[selectedThreadKey] ?? null) : null;
  const view = mockViews.find((v) => v.key === activeView);
  const listTitle = view?.label ?? 'すべて';
  // 案件で絞っているときは実件数、そうでなければビューの総数を出す
  // （モックは 24 スレッドのうち先頭 6 件だけを持っている）
  const listCount = activeProjectTag ? threads.length : (view?.count ?? threads.length);

  /** アーカイブしたら、その位置にあった次のスレッドへ選択を送る */
  function handleArchive(key: string) {
    const index = threadKeys.indexOf(key);
    const after = threadKeys[index + 1] ?? threadKeys[index - 1];
    archiveThread(key);
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
                  onArchive={() => handleArchive(t.thread_key)}
                />
              ))}
            </ThreadList>
          }
          thread={
            <ThreadView
              thread={selected}
              onArchive={() => selectedThreadKey && handleArchive(selectedThreadKey)}
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
