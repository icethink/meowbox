import type { ReactNode } from 'react';

/** キーボードショートカットの表示。数値・時刻と同じく等幅で出す */
export function Kbd({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={`rounded-xs bg-kbd px-[5px] py-px font-mono text-2xs font-normal text-muted ${className}`}
    >
      {children}
    </kbd>
  );
}
