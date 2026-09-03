import type { ReactNode } from 'react';

/**
 * 小さいラベル。tone は「誰の情報か」で決める。
 *   neutral … 案件タグなど中立
 *   accent  … 人間側（未読数・期限）
 *   ai      … Claude 由来（抽出タスク数）
 *   ok      … 確定済み
 * accent と ai は絶対に混ぜない（誰が書いたか一目で分かるようにするため）。
 */
export type BadgeTone = 'neutral' | 'accent' | 'ai' | 'ok' | 'faint';

const toneClass: Record<BadgeTone, string> = {
  neutral: 'bg-chip text-muted',
  accent: 'bg-accent-bg text-accent',
  ai: 'bg-ai-bg-strong text-ai',
  ok: 'bg-ok-bg text-ok',
  faint: 'text-faint',
};

export function Badge({
  tone = 'neutral',
  mono = false,
  className = '',
  children,
}: {
  tone?: BadgeTone;
  mono?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-xs px-[6px] text-2xs ${
        mono ? 'font-mono' : ''
      } ${toneClass[tone]} ${className}`}
    >
      {children}
    </span>
  );
}
