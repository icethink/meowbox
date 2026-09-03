import { useMemo } from 'react';
import { AppShell } from './components/layout/AppShell';
import { Sidebar } from './components/sidebar/Sidebar';
import { ThreadList } from './components/list/ThreadList';
import { ThreadRow } from './components/list/ThreadRow';
import { ThreadView } from './components/thread/ThreadView';
import { mockThreadDetails, mockThreads } from './mock/threads';
import { mockViews } from './mock/accounts';
import { useAppStore } from './store/app';

export default function App() {
  const { selectedThreadKey, selectThread, archivedKeys, archiveThread, readKeys } = useAppStore();
  const activeProjectTag = useAppStore((s) => s.activeProjectTag);
  const activeView = useAppStore((s) => s.activeView);

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
  const selected = selectedThreadKey ? (mockThreadDetails[selectedThreadKey] ?? null) : null;
  const listTitle = mockViews.find((v) => v.key === activeView)?.label ?? 'すべて';

  /** アーカイブしたら、その位置にあった次のスレッドへ選択を送る */
  function handleArchive(key: string) {
    const index = threadKeys.indexOf(key);
    const after = threadKeys[index + 1] ?? threadKeys[index - 1];
    archiveThread(key);
    if (after) selectThread(after);
  }

  return (
    <AppShell
      sidebar={<Sidebar />}
      list={
        <ThreadList title={listTitle} threads={threads}>
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
      // TODO(P2): (5) でダイジェストを入れる
      panel={null}
    />
  );
}
