import { Check } from 'lucide-react';

export type CheckboxTone = 'neutral' | 'candidate' | 'danger';

/**
 * タスク行のチェックボックス。
 * 実線 = 確定したタスク、点線 = Claude が出した候補（まだ人間が認めていない）。
 * 枠線の見た目そのものが「誰が決めたか」を表すので、tone を勝手に変えない。
 */
export function Checkbox({
  checked = false,
  tone = 'neutral',
  size = 15,
  label,
  onChange,
}: {
  checked?: boolean;
  tone?: CheckboxTone;
  size?: number;
  label: string;
  onChange?: (next: boolean) => void;
}) {
  const toneClass: Record<CheckboxTone, string> = {
    neutral: 'border-solid border-muted',
    candidate: 'border-dashed border-ai-muted',
    danger: 'border-solid border-danger',
  };

  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange?.(!checked)}
      style={{ width: size, height: size }}
      className={`grid shrink-0 place-items-center rounded-xs border-[1.5px] transition-colors ${
        toneClass[tone]
      } ${checked ? 'bg-accent-bg' : ''}`}
    >
      {checked && <Check size={size - 5} strokeWidth={3} className="text-accent" />}
    </button>
  );
}
