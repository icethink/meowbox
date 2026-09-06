import { Search } from 'lucide-react';
import { useEffect, useState } from 'react';
import { getSyncStatus, listAccounts, listProjects, listViews, onSyncProgress } from '../../api';
import { useRefreshOnFocus } from '../../hooks/useRefreshOnFocus';
import { useAppStore } from '../../store/app';
import type { ProjectGroupView, SyncState, ViewItemView } from '../../types.ui';
import { SettingsModal } from '../settings/SettingsModal';
import { Kbd } from '../ui/Kbd';
import { Logo } from './Logo';
import { ProjectGroup } from './ProjectGroup';
import { SyncStatus } from './SyncStatus';
import { ViewItem } from './ViewItem';

function SectionLabel({ children }: { children: string }) {
  return (
    <div className="px-[8px] pb-[6px] text-xs font-medium tracking-caps text-faint">{children}</div>
  );
}

export function Sidebar() {
  const activeProjectTag = useAppStore((s) => s.activeProjectTag);
  const setActiveProjectTag = useAppStore((s) => s.setActiveProjectTag);
  const activeView = useAppStore((s) => s.activeView);
  const setActiveView = useAppStore((s) => s.setActiveView);
  const openPalette = useAppStore((s) => s.setCommandPaletteOpen);

  const [projects, setProjects] = useState<ProjectGroupView[]>([]);
  const [views, setViews] = useState<ViewItemView[]>([]);
  const [sync, setSync] = useState<{ state: SyncState; label: string }>({
    state: 'ok',
    label: '',
  });
  const [settingsOpen, setSettingsOpen] = useState(false);

  /** 案件・ビュー・同期状態だけを読み直す（未読件数が変わったとき） */
  async function reloadSidebar() {
    const [p, v, s] = await Promise.all([listProjects(), listViews(), getSyncStatus()]);
    setProjects(p);
    setViews(v);
    setSync(s);
  }

  useEffect(() => {
    void reloadSidebar();
  }, []);

  // ウィンドウにフォーカスが戻ったら案件・ビュー・同期状態を読み直す
  useRefreshOnFocus(reloadSidebar);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await onSyncProgress((e) => {
        if (!e.done) {
          setSync({
            state: 'syncing',
            label: e.total > 0 ? `同期中 ${e.fetched} / ${e.total}` : '同期中',
          });
          return;
        }
        if (e.error) {
          setSync({ state: 'error', label: `エラー: ${e.error}` });
          return;
        }
        void reloadSidebar();
      });
    })();
    return () => unlisten?.();
  }, []);

  /** 設定モーダルでの変更後。アカウント一覧も併せて読み直しておく */
  function handleSettingsChanged() {
    void reloadSidebar();
    void listAccounts();
  }

  return (
    <nav
      aria-label="案件とビュー"
      className="flex w-[var(--w-sidebar)] shrink-0 flex-col border-r border-line bg-sidebar"
    >
      <div className="flex items-center gap-[9px] px-[14px] pt-[14px] pb-[10px]">
        <Logo />
        <span className="text-lg font-bold tracking-w2">Meowbox</span>
      </div>

      <button
        type="button"
        onClick={() => openPalette(true)}
        className="mx-[12px] mt-[2px] mb-[12px] flex items-center gap-[7px] rounded-token border border-line bg-selected px-[9px] py-[6px] text-placeholder transition-colors hover:bg-hover"
      >
        <Search size={13} strokeWidth={2} />
        <span className="flex-1 text-left text-12">検索</span>
        {/* Windows なので ⌘ ではなく Ctrl で出す */}
        <Kbd>Ctrl K</Kbd>
      </button>

      <div className="flex flex-1 flex-col gap-[16px] overflow-y-auto px-[8px]">
        <section>
          <SectionLabel>案件</SectionLabel>
          <div className="flex flex-col gap-px">
            {projects.map((p) => (
              <ProjectGroup
                key={p.tag}
                project={p}
                selected={activeProjectTag === p.tag}
                onSelect={() => setActiveProjectTag(activeProjectTag === p.tag ? null : p.tag)}
              />
            ))}
          </div>
        </section>

        <section>
          <SectionLabel>ビュー</SectionLabel>
          <div className="flex flex-col gap-px">
            {views.map((v) => (
              <ViewItem
                key={v.key}
                view={v}
                active={activeView === v.key}
                onSelect={() => setActiveView(v.key)}
              />
            ))}
          </div>
        </section>
      </div>

      <SyncStatus
        state={sync.state}
        label={sync.label}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onChanged={handleSettingsChanged}
      />
    </nav>
  );
}
