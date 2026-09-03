import { Flag, MoreHorizontal } from 'lucide-react';
import type { ThreadDetail } from '../../types.ui';
import { Badge } from '../ui/Badge';
import { IconButton } from '../ui/IconButton';

export function ThreadHeader({
  thread,
  onArchive,
}: {
  thread: ThreadDetail;
  onArchive: () => void;
}) {
  const name = thread.counterpart.name ?? thread.counterpart.email;

  return (
    <header className="flex items-center gap-[10px] border-b border-line px-[20px] pt-[14px] pb-[12px]">
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-xl font-bold">{thread.subject}</h1>
        <div className="mt-[3px] flex items-center gap-[8px]">
          <Badge tone="neutral" className="px-[7px] py-px">
            {thread.project_tag}
          </Badge>
          <span className="truncate text-11 text-faint">
            {name} &lt;<span className="font-mono">{thread.counterpart.email}</span>&gt; との{' '}
            {thread.message_count} 通
          </span>
        </div>
      </div>
      <div className="flex shrink-0 gap-[6px]">
        <IconButton onClick={onArchive}>アーカイブ</IconButton>
        <IconButton aria-label="フラグ">
          <Flag size={12} strokeWidth={1.8} />
        </IconButton>
        <IconButton aria-label="その他">
          <MoreHorizontal size={12} strokeWidth={1.8} />
        </IconButton>
      </div>
    </header>
  );
}
