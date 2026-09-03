import type { ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import type { ThreadListItem } from '../../types.ui';
import { useAppStore } from '../../store/app';

function FilterChip({
  label,
  active = false,
  dropdown = false,
}: {
  label: string;
  active?: boolean;
  dropdown?: boolean;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      className={`inline-flex items-center gap-[3px] rounded-pill border px-[9px] py-[2px] text-11 transition-colors ${
        active
          ? 'border-accent-ring bg-accent-bg-soft text-accent'
          : 'border-line-strong text-muted hover:bg-hover'
      }`}
    >
      {label}
      {dropdown && <ChevronDown size={10} strokeWidth={2} aria-hidden="true" />}
    </button>
  );
}

function ShortcutHint({ keys, label }: { keys: string; label: string }) {
  return (
    <span className="whitespace-nowrap">
      {keys} {label}
    </span>
  );
}

export function ThreadList({
  title,
  threads,
  children,
}: {
  title: string;
  threads: ThreadListItem[];
  /** 行の描画は呼び出し側に任せる（選択とキーボード操作を App 側で持つため） */
  children: ReactNode;
}) {
  const activeProjectTag = useAppStore((s) => s.activeProjectTag);

  return (
    <section
      aria-label="スレッド一覧"
      className="flex w-[var(--w-list)] shrink-0 flex-col border-r border-line bg-base"
    >
      <header className="flex flex-col gap-[9px] border-b border-line px-[14px] pt-[12px] pb-[10px]">
        <div className="flex items-baseline gap-[8px]">
          <h2 className="text-lg font-bold">{title}</h2>
          <span className="font-mono text-11 text-faint">{threads.length} スレッド</span>
        </div>
        <div className="flex gap-[6px]">
          <FilterChip
            label={activeProjectTag ?? '案件'}
            active={activeProjectTag !== null}
            dropdown
          />
          <FilterChip label="未読" />
          <FilterChip label="要対応" active />
          <FilterChip label="期間" dropdown />
        </div>
      </header>

      <div className="flex-1 overflow-y-auto">{children}</div>

      <footer className="flex gap-[12px] border-t border-line px-[14px] py-[7px] font-mono text-xs text-disabled">
        <ShortcutHint keys="j/k" label="移動" />
        <ShortcutHint keys="e" label="アーカイブ" />
        <ShortcutHint keys="r" label="返信" />
      </footer>
    </section>
  );
}
