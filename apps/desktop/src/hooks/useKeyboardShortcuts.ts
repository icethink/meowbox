import { useEffect } from 'react';
import { REPLY_INPUT_ID } from '../components/thread/ReplyBox';
import { useAppStore } from '../store/app';

/** 入力中はショートカットを拾わない（返信本文に "e" が消える、を防ぐ） */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || target.isContentEditable;
}

export interface ShortcutContext {
  /** 表示中の一覧（アーカイブ済みを除いたもの） */
  threadKeys: string[];
  selectedThreadKey: string | null;
}

/**
 * 一覧のキーボード操作。
 *   j / k … 次 / 前のスレッド
 *   e     … アーカイブ（一覧から消える）
 *   r     … 返信欄にフォーカス
 *   Ctrl+\ … 右パネル
 *   Ctrl+K … コマンドパレット
 *
 * macOS の ⌘ に相当するものは Windows では Ctrl として扱う。
 */
export function useKeyboardShortcuts({ threadKeys, selectedThreadKey }: ShortcutContext) {
  const selectThread = useAppStore((s) => s.selectThread);
  const archiveThread = useAppStore((s) => s.archiveThread);
  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel);
  const setCommandPaletteOpen = useAppStore((s) => s.setCommandPaletteOpen);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;

      if (mod && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setCommandPaletteOpen(true);
        return;
      }
      if (mod && e.key === '\\') {
        e.preventDefault();
        toggleRightPanel();
        return;
      }
      if (mod || e.altKey || isTyping(e.target)) return;

      const index = selectedThreadKey ? threadKeys.indexOf(selectedThreadKey) : -1;

      switch (e.key) {
        case 'j': {
          e.preventDefault();
          const next = threadKeys[Math.min(index + 1, threadKeys.length - 1)];
          if (next) selectThread(next);
          break;
        }
        case 'k': {
          e.preventDefault();
          const prev = threadKeys[Math.max(index - 1, 0)];
          if (prev) selectThread(prev);
          break;
        }
        case 'e': {
          if (!selectedThreadKey) break;
          e.preventDefault();
          // アーカイブしたら、その位置にあった次のスレッドへ送る
          const after = threadKeys[index + 1] ?? threadKeys[index - 1];
          archiveThread(selectedThreadKey);
          if (after) selectThread(after);
          break;
        }
        case 'r': {
          e.preventDefault();
          document.getElementById(REPLY_INPUT_ID)?.focus();
          break;
        }
      }
    }

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [
    threadKeys,
    selectedThreadKey,
    selectThread,
    archiveThread,
    toggleRightPanel,
    setCommandPaletteOpen,
  ]);
}
