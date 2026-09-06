import { Archive, Flag, Paperclip, SquareCheck } from 'lucide-react';
import { splitSubjectMarker } from '../../lib/subject';
import type { ThreadListItem } from '../../types.ui';
import { Badge } from '../ui/Badge';

/** 左端のアクセントバー。要対応の度合いを人間側の色で出す */
const urgencyBar = {
  urgent: 'bg-danger',
  action: 'bg-accent',
  none: 'bg-transparent',
} as const;

export function ThreadRow({
  thread,
  selected,
  onSelect,
  onArchive,
}: {
  thread: ThreadListItem;
  selected: boolean;
  onSelect: () => void;
  onArchive: () => void;
}) {
  const { marker, rest } = splitSubjectMarker(thread.subject);
  const bar = urgencyBar[thread.urgency ?? 'none'];

  return (
    <div
      className={`group relative flex border-b border-line-faint transition-colors ${
        selected ? 'bg-accent-bg-subtle' : 'hover:bg-subtle'
      } ${thread.is_muted ? 'opacity-55' : ''}`}
    >
      <span className={`w-[var(--w-accent-bar)] shrink-0 ${bar}`} aria-hidden="true" />

      <button
        type="button"
        onClick={onSelect}
        aria-current={selected ? 'true' : undefined}
        className="flex min-w-0 flex-1 flex-col gap-[3px] py-[10px] pr-[12px] pl-[10px] text-left"
      >
        <span className="flex items-center gap-[7px]">
          <span
            className={`size-[7px] shrink-0 rounded-full ${thread.is_unread ? 'bg-accent' : ''}`}
            aria-hidden="true"
          />
          <span
            className={`min-w-0 flex-1 truncate text-base ${
              thread.is_unread ? 'font-bold text-primary' : 'font-medium text-secondary'
            }`}
          >
            {thread.from_name}
          </span>
          {thread.has_attachments && (
            <Paperclip
              size={11}
              strokeWidth={2}
              className="shrink-0 text-muted"
              aria-label="添付"
            />
          )}
          <span className="shrink-0 font-mono text-xs text-faint group-hover:invisible">
            {thread.time_label}
          </span>
        </span>

        <span
          className={`truncate text-base ${
            thread.is_unread ? 'font-bold text-primary' : 'text-secondary'
          }`}
        >
          {marker && <span className="text-danger">{marker}</span>}
          {rest}
        </span>

        {thread.ai_snippet ? (
          // Claude が付けた 1 行要約。人間が書いた本文と混ざらないよう --ai 系で出す
          <span className="truncate text-sm text-ai-muted">{thread.ai_snippet}</span>
        ) : (
          thread.snippet && (
            // 要約がまだ無いときは本文の抜粋を出す。人間が書いた文章なので --ai は使わない
            <span className="truncate text-sm text-muted">{thread.snippet}</span>
          )
        )}

        <span className="mt-px flex items-center gap-[6px]">
          <Badge tone="neutral">{thread.project_tag}</Badge>
          {thread.task_count > 0 && (
            <Badge tone="ai" mono className="gap-[3px]">
              <SquareCheck size={9} strokeWidth={2.4} />
              {thread.task_count}
            </Badge>
          )}
        </span>
      </button>

      {/* ホバー中だけ出るクイックアクション。日時の上に重ねて出すので行の高さは動かない */}
      <div className="absolute top-[8px] right-[10px] hidden items-center gap-[2px] group-hover:flex">
        <button
          type="button"
          aria-label="アーカイブ"
          title="アーカイブ (e)"
          onClick={onArchive}
          className="rounded-xs p-[3px] text-muted transition-colors hover:bg-selected hover:text-primary"
        >
          <Archive size={13} strokeWidth={1.8} />
        </button>
        <button
          type="button"
          aria-label="フラグ"
          title="フラグ"
          className="rounded-xs p-[3px] text-muted transition-colors hover:bg-selected hover:text-accent"
        >
          <Flag size={13} strokeWidth={1.8} />
        </button>
      </div>
    </div>
  );
}
