import { useEffect } from 'react';
import { useAppStore } from '../../store/app';

export function Toast() {
  const message = useAppStore((s) => s.toast);
  const dismiss = useAppStore((s) => s.dismissToast);

  useEffect(() => {
    if (!message) return;
    const id = window.setTimeout(dismiss, 2600);
    return () => window.clearTimeout(id);
  }, [message, dismiss]);

  if (!message) return null;

  return (
    <div
      role="status"
      className="fixed bottom-[20px] left-1/2 z-50 -translate-x-1/2 rounded-token border border-line-strong bg-elevated px-[16px] py-[8px] text-base text-primary shadow-lg"
    >
      {message}
    </div>
  );
}
