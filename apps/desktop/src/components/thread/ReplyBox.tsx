import { useState } from 'react';
import { Sparkles } from 'lucide-react';
import {
  aiDraftAvailable,
  createDraft,
  generateAiDraft,
  sendAvailable,
  sendDraft,
} from '../../api';
import { useAppStore } from '../../store/app';
import { Modal } from '../ui/Modal';

/** `r` キーからフォーカスするために固定 id を振る */
export const REPLY_INPUT_ID = 'reply-input';

/**
 * 返信欄。
 *
 * 送信ボタンのラベルは常に「確認して送信」。AI が書いた下書きを人間が
 * 一度も読まずに送ってしまうのが一番まずいので、未編集のまま送ろうとしたときだけ
 * 確認ダイアログを挟む。
 */
export function ReplyBox({
  threadKey,
  placeholder,
  inReplyTo,
}: {
  threadKey: string;
  placeholder: string;
  /** 返信元のメッセージ id。下書きの保存先が決まらないので null なら保存しない */
  inReplyTo: number | null;
}) {
  const body = useAppStore((s) => s.replyBody);
  const isUneditedAiDraft = useAppStore((s) => s.replyIsUneditedAiDraft);
  const setBody = useAppStore((s) => s.setReplyBody);
  const insertAiDraft = useAppStore((s) => s.insertAiDraft);
  const clearReply = useAppStore((s) => s.clearReply);
  const showToast = useAppStore((s) => s.showToast);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [savingDraft, setSavingDraft] = useState(false);

  async function handleGenerate() {
    setGenerating(true);
    try {
      const draft = await generateAiDraft(threadKey);
      insertAiDraft(draft);
      if (inReplyTo !== null) {
        try {
          await createDraft({ in_reply_to: inReplyTo, body: draft });
        } catch (err) {
          console.error(err);
          showToast('下書きの保存に失敗しました');
        }
      }
    } finally {
      setGenerating(false);
    }
  }

  async function reallySend() {
    try {
      await sendDraft({ thread_key: threadKey, body });
      clearReply();
      setConfirmOpen(false);
      showToast('送信しました');
    } catch (err) {
      console.error(err);
      setConfirmOpen(false);
      showToast('送信に失敗しました');
    }
  }

  function handleSend() {
    // 送信はまだ実装していない（CLAUDE.md の安全境界）。ボタンも disabled にしているが、
    // 念のためここでも到達させない
    if (!sendAvailable) return;
    if (body.trim() === '') return;
    if (isUneditedAiDraft) {
      setConfirmOpen(true);
      return;
    }
    void reallySend();
  }

  async function handleSaveDraft() {
    if (inReplyTo === null) return;
    if (body.trim() === '') return;
    setSavingDraft(true);
    try {
      await createDraft({ in_reply_to: inReplyTo, body });
      showToast('下書きを保存しました');
    } catch (err) {
      console.error(err);
      showToast('下書きの保存に失敗しました');
    } finally {
      setSavingDraft(false);
    }
  }

  return (
    <div className="flex flex-col gap-[9px] border-t border-line bg-surface px-[20px] pt-[12px] pb-[14px]">
      {isUneditedAiDraft && (
        <div className="flex items-center gap-[6px] text-11 text-ai">
          <Sparkles size={11} strokeWidth={2} aria-hidden="true" />
          Claude の下書き — 編集してから送信してください
        </div>
      )}

      <textarea
        id={REPLY_INPUT_ID}
        value={body}
        onChange={(e) => setBody(e.target.value)}
        placeholder={placeholder}
        aria-label="返信本文"
        rows={body ? 5 : 1}
        // 空のときの高さはデザインどおり 62px（本文 1 行 + 上下 10px + 枠）
        className={`selectable min-h-[62px] resize-none rounded-md border px-[12px] py-[10px] text-base leading-body outline-none placeholder:text-faint ${
          isUneditedAiDraft
            ? 'border-ai-line-strong bg-ai-bg text-ai-text'
            : 'border-line-strong text-primary'
        }`}
      />

      <div className="flex items-center gap-[8px]">
        <button
          type="button"
          onClick={handleGenerate}
          disabled={generating || !aiDraftAvailable}
          title={aiDraftAvailable ? undefined : 'Claude の下書きは P1 で対応'}
          className="inline-flex items-center gap-[6px] rounded-token border border-ai-line-strong px-[11px] py-[5px] text-sm text-ai transition-colors hover:bg-ai-bg-hover disabled:opacity-60"
        >
          <Sparkles size={12} strokeWidth={2} aria-hidden="true" />
          AI で下書き
        </button>
        <span className="text-11 text-faint">Tab で挿入 · 編集してから送信</span>
        <button
          type="button"
          onClick={() => void handleSaveDraft()}
          disabled={inReplyTo === null || savingDraft || body.trim() === ''}
          className="ml-auto rounded-token border border-accent px-[14px] py-[6px] text-12 font-bold text-accent transition-colors hover:bg-accent-bg disabled:opacity-50"
        >
          下書きを保存
        </button>
        <button
          type="button"
          onClick={handleSend}
          disabled={!sendAvailable}
          className="rounded-token bg-accent px-[16px] py-[6px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover disabled:opacity-50"
        >
          確認して送信
        </button>
      </div>
      {!sendAvailable && (
        <span className="text-11 text-faint">送信は未対応です（下書きの保存まで）</span>
      )}

      <Modal
        open={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        labelledBy="confirm-ai-send-title"
        className="w-[420px] p-[18px]"
      >
        <h2 id="confirm-ai-send-title" className="text-md font-bold">
          AI の下書きをそのまま送信しますか？
        </h2>
        <p className="mt-[8px] text-base leading-relaxed text-muted">
          本文は Claude が生成したまま、まだ一度も編集されていません。
          相手に届く前に内容を確認することをおすすめします。
        </p>
        <div className="mt-[16px] flex justify-end gap-[8px]">
          <button
            type="button"
            onClick={() => setConfirmOpen(false)}
            className="rounded-token border border-line-soft px-[14px] py-[6px] text-12 text-muted transition-colors hover:bg-selected"
          >
            戻って編集する
          </button>
          <button
            type="button"
            onClick={() => void reallySend()}
            className="rounded-token bg-accent px-[14px] py-[6px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover"
          >
            このまま送信
          </button>
        </div>
      </Modal>
    </div>
  );
}
