import { useState } from 'react';
import { Paperclip } from 'lucide-react';
import { openAttachment } from '../../api';
import { useAppStore } from '../../store/app';
import type { ThreadMessageView } from '../../types.ui';
import { RichText } from './RichText';

export function MessageItem({ message }: { message: ThreadMessageView }) {
  const [quoteOpen, setQuoteOpen] = useState(false);
  const [openingId, setOpeningId] = useState<number | null>(null);
  const showToast = useAppStore((s) => s.showToast);

  async function handleOpenAttachment(id: number) {
    setOpeningId(id);
    try {
      await openAttachment(id);
    } catch (err) {
      console.error(err);
      showToast('添付を開けませんでした');
    } finally {
      setOpeningId(null);
    }
  }

  return (
    <article className="flex flex-col gap-[6px]">
      <header className="flex items-baseline gap-[8px]">
        <span
          className="grid size-[26px] shrink-0 self-center place-items-center rounded-full bg-avatar text-11 font-bold text-muted"
          aria-hidden="true"
        >
          {message.initial}
        </span>
        <span className="text-base font-bold">{message.from.name ?? message.from.email}</span>
        <span className="font-mono text-11 text-faint">{message.time_label}</span>
      </header>

      <div
        className={`selectable ml-[34px] text-base leading-body ${
          message.is_latest ? 'text-body-strong' : 'text-body'
        }`}
      >
        <RichText spans={message.body} strongClassName="font-bold" />

        {message.quoted_lines > 0 && (
          <div className="mt-[8px]">
            <button
              type="button"
              onClick={() => setQuoteOpen((v) => !v)}
              aria-expanded={quoteOpen}
              className="rounded-sm border border-line-soft px-[10px] py-[3px] text-11 text-faint transition-colors hover:text-muted"
            >
              {quoteOpen ? '引用を隠す' : `… 引用 ${message.quoted_lines} 行を表示`}
            </button>
            {quoteOpen && (
              <pre className="selectable mt-[8px] overflow-x-auto border-l-2 border-line-soft pl-[10px] text-11 leading-relaxed whitespace-pre-wrap text-faint">
                {message.quoted_text}
              </pre>
            )}
          </div>
        )}

        {message.attachments.length > 0 && (
          <div className="mt-[8px] flex flex-wrap gap-[6px]">
            {message.attachments.map((a) => (
              <button
                key={a.id}
                type="button"
                title={`${a.filename} (${a.size_label})`}
                disabled={openingId === a.id}
                onClick={() => void handleOpenAttachment(a.id)}
                className="inline-flex items-center gap-[5px] rounded-sm border border-line-strong px-[10px] py-[3px] font-mono text-11 text-muted transition-colors hover:bg-selected hover:text-primary disabled:opacity-60"
              >
                <Paperclip size={11} strokeWidth={2} aria-hidden="true" />
                {a.filename}
                <span className="text-faint">{a.size_label}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </article>
  );
}
