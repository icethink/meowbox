import { CheckSquare, Circle, Diamond, Dot, Flag, PencilLine } from 'lucide-react';
import type { ViewItemView, ViewKey } from '../../types.ui';

const icons: Record<ViewKey, typeof Circle> = {
  all: Dot,
  unread: Circle,
  action: Diamond,
  flagged: Flag,
  tasks: CheckSquare,
  drafts: PencilLine,
};

/** 件数の色。人間側の指標は --accent、Claude が作ったタスクは --ai */
const toneClass = {
  accent: 'text-accent',
  ai: 'text-ai',
  null: 'text-faint',
} as const;

export function ViewItem({
  view,
  active,
  onSelect,
}: {
  view: ViewItemView;
  active: boolean;
  onSelect: () => void;
}) {
  const Icon = icons[view.key];
  const tone = toneClass[view.tone ?? 'null'];

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={active ? 'page' : undefined}
      className={`flex w-full items-center gap-[8px] rounded-token px-[8px] py-[5px] text-base transition-colors ${
        active ? 'bg-selected font-medium text-primary' : 'text-muted hover:bg-hover'
      }`}
    >
      <Icon
        size={13}
        strokeWidth={2}
        className={view.tone ? tone : 'text-muted'}
        fill={view.key === 'all' || view.key === 'action' ? 'currentColor' : 'none'}
      />
      <span className="flex-1 text-left">{view.label}</span>
      <span className={`font-mono text-xs ${tone}`}>{view.count}</span>
    </button>
  );
}
