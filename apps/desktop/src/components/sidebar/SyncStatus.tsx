import { Settings } from 'lucide-react';
import type { SyncState } from '../../types.ui';

const dotColor: Record<SyncState, string> = {
  ok: 'bg-ok',
  syncing: 'bg-ok',
  warn: 'bg-warn',
  error: 'bg-danger',
};

export function SyncStatus({ state, label }: { state: SyncState; label: string }) {
  return (
    <div className="flex items-center gap-[8px] border-t border-line px-[14px] py-[10px] text-11 text-faint">
      <span className={`size-[6px] shrink-0 rounded-full ${dotColor[state]}`} />
      <span className="flex-1 truncate">{label}</span>
      <button
        type="button"
        aria-label="設定"
        className="text-faint transition-colors hover:text-muted"
      >
        <Settings size={14} strokeWidth={1.8} />
      </button>
    </div>
  );
}
