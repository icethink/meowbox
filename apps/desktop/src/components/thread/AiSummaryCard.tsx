import { RotateCcw } from 'lucide-react';
import type { AiSummary } from '../../types.ui';
import { RichText } from './RichText';

/**
 * Claude が作った要約。人間が書いた本文と取り違えないよう、
 * 面・枠・文字色をすべて --ai 系に寄せ、見出しに生成元と生成時刻を出す。
 */
export function AiSummaryCard({
  summary,
  onRegenerate,
}: {
  summary: AiSummary;
  onRegenerate?: () => void;
}) {
  return (
    <section className="flex flex-col gap-[8px] rounded-md border border-ai-line bg-ai-bg px-[14px] py-[12px]">
      <header className="flex items-center gap-[8px]">
        <span className="size-[6px] shrink-0 rounded-full bg-ai" aria-hidden="true" />
        <h3 className="text-11 font-bold tracking-w4 text-ai">Claude による要約</h3>
        <span className="font-mono text-2xs text-faint">{summary.generated_label}</span>
        <button
          type="button"
          onClick={onRegenerate}
          className="ml-auto inline-flex items-center gap-[4px] rounded-sm border border-ai-line-strong px-[8px] py-px text-xs text-ai transition-colors hover:bg-ai-bg-hover"
        >
          <RotateCcw size={10} strokeWidth={2} aria-hidden="true" />
          再生成
        </button>
      </header>
      <p className="selectable text-base leading-loose text-ai-text-muted">
        <RichText spans={summary.body} strongClassName="font-medium text-ai-text" />
      </p>
    </section>
  );
}
