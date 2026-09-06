import { useCallback, useEffect, useState } from 'react';
import { X } from 'lucide-react';
import { getDigest } from '../../api';
import { useRefreshOnFocus } from '../../hooks/useRefreshOnFocus';
import type { Digest, DigestItem } from '../../types.ui';
import { useAppStore } from '../../store/app';
import { Checkbox } from '../ui/Checkbox';
import { RichText } from '../thread/RichText';

const dueToneClass = {
  danger: 'text-danger',
  accent: 'text-accent',
  neutral: 'text-muted',
} as const;

function DigestRow({ item }: { item: DigestItem }) {
  const decisions = useAppStore((s) => s.taskDecisions);
  const decide = useAppStore((s) => s.decideTask);
  const done = decisions[item.id] === 'done';

  if (item.is_candidate) {
    return (
      <div className="flex items-center gap-[8px] rounded-token border-[1.5px] border-dashed border-ai-line-strong px-[10px] py-[7px]">
        <Checkbox tone="candidate" size={13} label={`${item.title}（候補）`} />
        <span className="flex-1 truncate text-sm text-ai-text-muted">{item.title}</span>
        <span className="text-9-5 text-ai">候補</span>
      </div>
    );
  }

  const urgent = item.due_tone === 'danger';
  return (
    <div
      className={`flex items-center gap-[8px] rounded-token border px-[10px] py-[7px] ${
        urgent ? 'border-danger-line bg-danger-bg' : 'border-line-soft bg-subtle'
      }`}
    >
      <Checkbox
        checked={done}
        tone={urgent ? 'danger' : 'neutral'}
        size={13}
        label={`${item.title} を完了にする`}
        onChange={(next) => decide(item.id, next ? 'done' : 'confirmed')}
      />
      <span className={`flex-1 truncate text-sm ${done ? 'text-faint line-through' : ''}`}>
        {item.title}
      </span>
      {item.due_label && (
        <span className={`font-mono text-2xs ${dueToneClass[item.due_tone]}`}>
          {item.due_label}
        </span>
      )}
    </div>
  );
}

export function DigestPanel() {
  const close = useAppStore((s) => s.setRightPanelOpen);
  const [digest, setDigest] = useState<Digest | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const result = await getDigest();
      setDigest(result);
    } catch (err) {
      // 本文やアドレスをログに出さないよう、エラーだけ記録して空状態にする
      console.error('failed to load digest', err);
      setDigest(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // ウィンドウにフォーカスが戻ったらダイジェストを読み直す
  useRefreshOnFocus(load);

  return (
    <aside
      aria-label="今日のダイジェスト"
      className="flex w-[var(--w-panel)] shrink-0 flex-col overflow-y-auto border-l border-line bg-sidebar"
    >
      <header className="flex items-center gap-[8px] border-b border-line px-[16px] pt-[14px] pb-[10px]">
        <h2 className="text-md font-bold">今日のダイジェスト</h2>
        <span className="font-mono text-2xs text-faint">{digest?.date_label ?? ''}</span>
        <button
          type="button"
          aria-label="ダイジェストを閉じる"
          onClick={() => close(false)}
          className="ml-auto text-faint transition-colors hover:text-muted"
        >
          <X size={13} strokeWidth={2} />
        </button>
      </header>

      {loading ? (
        <p className="px-[16px] py-[12px] text-xs text-faint">読み込み中…</p>
      ) : (
        <>
          <div className="flex flex-col gap-[8px] px-[12px] pt-[12px] pb-[4px]">
            {/* Claude が書いたまとめ。人間が書いたものと混ざらないよう --ai の面に載せる */}
            <section className="flex flex-col gap-[5px] rounded-[7px] border border-ai-line bg-ai-bg px-[12px] py-[10px]">
              <h3 className="flex items-center gap-[6px] text-xs font-bold text-ai">
                <span className="size-[5px] shrink-0 rounded-full bg-ai" aria-hidden="true" />
                Claude のまとめ
              </h3>
              {digest && digest.summary.length > 0 ? (
                <p className="selectable text-sm leading-relaxed text-ai-text-muted">
                  <RichText spans={digest.summary} strongClassName="font-medium text-ai-text" />
                </p>
              ) : (
                <p className="text-sm leading-relaxed text-ai-text-muted">
                  今日のまとめはまだありません
                  <br />
                  <span className="text-xs text-faint">
                    Claude が MCP 経由で書き込むとここに出ます
                  </span>
                </p>
              )}
            </section>
          </div>

          <div className="flex flex-col gap-[14px] px-[12px] pt-[8px] pb-[16px]">
            {digest && digest.groups.length > 0 ? (
              digest.groups.map((group) => (
                <section key={group.project_tag} className="flex flex-col gap-[5px]">
                  <h3 className="px-[2px] text-xs font-medium tracking-caps text-faint">
                    {group.project_tag}
                  </h3>
                  {group.items.map((item) => (
                    <DigestRow key={item.id} item={item} />
                  ))}
                </section>
              ))
            ) : (
              <p className="px-[2px] text-xs text-faint">抽出されたタスクはありません</p>
            )}
          </div>
        </>
      )}
    </aside>
  );
}
