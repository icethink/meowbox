import { useEffect, type ReactNode } from 'react';

/** Esc で閉じられる素朴なモーダル。フォーカストラップは P5 のコマンドパレットで入れる */
export function Modal({
  open,
  onClose,
  labelledBy,
  children,
  className = '',
}: {
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  children: ReactNode;
  className?: string;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-start justify-items-center bg-app/70 pt-[18vh]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        onClick={(e) => e.stopPropagation()}
        className={`rounded-md border border-line-strong bg-elevated shadow-2xl ${className}`}
      >
        {children}
      </div>
    </div>
  );
}
