import { useMemo } from 'react';
import { AppShell } from './components/layout/AppShell';
import { Sidebar } from './components/sidebar/Sidebar';
import { ThreadList } from './components/list/ThreadList';
import { ThreadRow } from './components/list/ThreadRow';
import { mockThreads } from './mock/threads';
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

  const listTitle = mockViews.find((v) => v.key === activeView)?.label ?? 'すべて';

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
              onArchive={() => archiveThread(t.thread_key)}
            />
          ))}
        </ThreadList>
      }
      // TODO(P2): (4) でスレッド表示、(5) でダイジェストを入れる
      thread={<section className="flex-1 bg-elevated" />}
      panel={null}
    />
  );
}
