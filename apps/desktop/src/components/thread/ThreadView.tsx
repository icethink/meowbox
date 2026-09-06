import type { ThreadDetail } from '../../types.ui';
import { AiSummaryCard } from './AiSummaryCard';
import { ExtractedTasks } from './ExtractedTasks';
import { MessageItem } from './MessageItem';
import { ReplyBox } from './ReplyBox';
import { ThreadHeader } from './ThreadHeader';

export function ThreadView({
  thread,
  onArchive,
}: {
  thread: ThreadDetail | null;
  onArchive: () => void;
}) {
  if (!thread) {
    return (
      <section className="grid flex-1 place-items-center bg-elevated text-base text-faint">
        スレッドを選択してください
      </section>
    );
  }

  return (
    <section aria-label="スレッド" className="flex min-w-0 flex-1 flex-col bg-elevated">
      <ThreadHeader thread={thread} onArchive={onArchive} />

      <div className="flex flex-1 flex-col gap-[14px] overflow-y-auto px-[20px] py-[16px]">
        <AiSummaryCard summary={thread.summary} />
        <ExtractedTasks tasks={thread.tasks} />
        <div className="mt-[4px] flex flex-col gap-[12px]">
          {thread.messages.map((m) => (
            <MessageItem key={m.id} message={m} />
          ))}
        </div>
      </div>

      <ReplyBox
        threadKey={thread.thread_key}
        placeholder={thread.reply_placeholder}
        inReplyTo={thread.messages.at(-1)?.id ?? null}
      />
    </section>
  );
}
