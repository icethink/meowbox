import type { ProjectGroupView, SyncState } from '../../types.ui';

const syncColor: Record<SyncState, string> = {
  ok: 'bg-ok',
  syncing: 'bg-ok',
  warn: 'bg-warn',
  error: 'bg-danger',
};

const syncLabel: Record<SyncState, string> = {
  ok: '同期済み',
  syncing: '同期中',
  warn: '同期に遅れがある',
  error: '同期エラー',
};

/**
 * 案件 1 つ = project_tag 1 つ。1 案件に複数のアドレスが配布されることがあるので
 * 所属アカウントを全部並べる（Meowbox がそもそも解こうとしている問題そのもの）。
 */
export function ProjectGroup({
  project,
  selected,
  onSelect,
}: {
  project: ProjectGroupView;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={`flex w-full flex-col gap-[2px] rounded-token px-[8px] py-[6px] text-left transition-colors ${
        selected ? 'bg-selected' : 'hover:bg-hover'
      }`}
    >
      <span className="flex items-center gap-[7px]">
        <span
          className={`size-[6px] shrink-0 rounded-full ${syncColor[project.sync]}`}
          title={syncLabel[project.sync]}
        />
        <span className="flex-1 truncate text-base font-medium text-primary">{project.tag}</span>
        {/* 要対応を抱えている案件だけ --accent で強調する。ただの未読は目立たせない */}
        {project.has_action ? (
          <span className="rounded-[8px] bg-accent-bg px-[6px] font-mono text-xs text-accent">
            {project.unread}
          </span>
        ) : (
          <span className="font-mono text-xs text-faint">{project.unread}</span>
        )}
      </span>
      {project.accounts.map((a) => (
        <span key={a.id} className="truncate pl-[13px] font-mono text-xs text-faint">
          {a.email}
        </span>
      ))}
    </button>
  );
}
