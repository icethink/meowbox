import type { Task } from '../../types';
import { formatDue, isConfident } from '../../lib/tasks';
import { Badge } from '../ui/Badge';
import { Checkbox } from '../ui/Checkbox';

/**
 * Claude が抽出したタスク 1 件。
 *
 * 確定 = 実線枠・チェックボックス・期日。人間が受け入れたもの。
 * 候補 = 点線枠・確度の表示・確定/却下ボタン。まだ Claude の言い分でしかないもの。
 * この 2 状態の見た目の差が「誰が決めたか」を表すので、枠線と色を混ぜない。
 */
export function TaskRow({
  task,
  checked = false,
  onToggle,
  onConfirm,
  onDismiss,
}: {
  task: Task;
  checked?: boolean;
  onToggle?: (next: boolean) => void;
  onConfirm?: () => void;
  onDismiss?: () => void;
}) {
  const confident = isConfident(task);

  if (confident) {
    return (
      <div className="flex items-center gap-[10px] rounded-[7px] border border-line-soft bg-subtle px-[12px] py-[8px]">
        <Checkbox checked={checked} onChange={onToggle} label={`${task.title} を完了にする`} />
        <span className={`flex-1 text-base ${checked ? 'text-faint line-through' : ''}`}>
          {task.title}
        </span>
        {task.due && <span className="font-mono text-11 text-accent">{formatDue(task.due)}</span>}
        <Badge tone="ok" className="px-[7px] py-px">
          確定
        </Badge>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-[10px] rounded-[7px] border-[1.5px] border-dashed border-ai-line-strong px-[12px] py-[8px]">
      <Checkbox tone="candidate" label={`${task.title}（候補）`} />
      <span className="flex-1 text-base text-ai-text-muted">
        {task.title}
        <span className="ml-[4px] text-2xs text-ai">
          候補 · 確度 {Math.round(task.confidence * 100)}%
        </span>
      </span>
      <button
        type="button"
        onClick={onConfirm}
        className="rounded-sm bg-ai-bg-strong px-[9px] py-[2px] text-xs text-ai transition-colors hover:bg-ai-bg-active"
      >
        確定
      </button>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-sm px-[9px] py-[2px] text-xs text-faint transition-colors hover:text-muted"
      >
        却下
      </button>
    </div>
  );
}
