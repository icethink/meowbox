import type { ButtonHTMLAttributes, ReactNode } from 'react';

/**
 * スレッドヘッダなどに並ぶ枠付きの小さいボタン。
 * 既定は無彩色。Claude 由来の操作は tone="ai" にする。
 */
export function IconButton({
  tone = 'neutral',
  className = '',
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: 'neutral' | 'ai';
  children: ReactNode;
}) {
  const toneClass =
    tone === 'ai'
      ? 'border-ai-line-strong text-ai hover:bg-ai-bg-hover'
      : 'border-line-soft text-muted hover:bg-selected hover:text-secondary';

  return (
    <button
      type="button"
      className={`inline-flex items-center gap-[6px] rounded-token border px-[8px] py-[5px] text-11 transition-colors ${toneClass} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
