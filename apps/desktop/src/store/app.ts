import { create } from 'zustand';
import type { ViewKey } from '../types.ui';

/** タスクに対してユーザーが下した判断。AI の confidence とは別に持つ */
export type TaskDecision = 'confirmed' | 'dismissed' | 'done';

interface AppState {
  // --- レイアウト ---
  /** 右パネル（ダイジェスト）。Ctrl+\ でトグル */
  rightPanelOpen: boolean;
  toggleRightPanel: () => void;
  setRightPanelOpen: (open: boolean) => void;

  /** Ctrl+K の空モーダル。中身は P5 */
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;

  // --- 選択 ---
  selectedThreadKey: string | null;
  selectThread: (key: string) => void;
  activeView: ViewKey;
  setActiveView: (view: ViewKey) => void;
  activeProjectTag: string | null;
  setActiveProjectTag: (tag: string | null) => void;

  // --- 一覧の状態 ---
  /** e でアーカイブしたスレッド。一覧から消えるだけで消去はしない */
  archivedKeys: string[];
  archiveThread: (key: string) => void;
  /** 開いたスレッドは既読にする */
  readKeys: string[];
  markRead: (key: string) => void;

  // --- タスク ---
  taskDecisions: Record<number, TaskDecision>;
  decideTask: (id: number, decision: TaskDecision) => void;

  // --- 返信欄 ---
  replyBody: string;
  /** 本文が Claude の下書きで、人間がまだ 1 文字も直していない状態 */
  replyIsUneditedAiDraft: boolean;
  setReplyBody: (body: string) => void;
  insertAiDraft: (body: string) => void;
  clearReply: () => void;

  // --- トースト ---
  toast: string | null;
  showToast: (message: string) => void;
  dismissToast: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  rightPanelOpen: true,
  toggleRightPanel: () => set((s) => ({ rightPanelOpen: !s.rightPanelOpen })),
  setRightPanelOpen: (open) => set({ rightPanelOpen: open }),

  commandPaletteOpen: false,
  setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),

  // デザインでは「Re: 見積の件」が選択されている
  selectedThreadKey: 'th-estimate',
  selectThread: (key) =>
    set((s) => ({
      selectedThreadKey: key,
      readKeys: s.readKeys.includes(key) ? s.readKeys : [...s.readKeys, key],
      // スレッドを切り替えたら書きかけの返信は捨てる（誤爆防止）
      replyBody: '',
      replyIsUneditedAiDraft: false,
    })),
  activeView: 'all',
  setActiveView: (view) => set({ activeView: view }),
  activeProjectTag: null,
  setActiveProjectTag: (tag) => set({ activeProjectTag: tag }),

  archivedKeys: [],
  archiveThread: (key) =>
    set((s) => ({
      archivedKeys: s.archivedKeys.includes(key) ? s.archivedKeys : [...s.archivedKeys, key],
    })),
  readKeys: [],
  markRead: (key) =>
    set((s) => ({ readKeys: s.readKeys.includes(key) ? s.readKeys : [...s.readKeys, key] })),

  taskDecisions: {},
  decideTask: (id, decision) =>
    set((s) => ({ taskDecisions: { ...s.taskDecisions, [id]: decision } })),

  replyBody: '',
  replyIsUneditedAiDraft: false,
  setReplyBody: (body) =>
    set((s) => ({
      replyBody: body,
      // 人間が 1 文字でも触ったら「AI の下書きそのまま」ではなくなる
      replyIsUneditedAiDraft: s.replyIsUneditedAiDraft && body === s.replyBody,
    })),
  insertAiDraft: (body) => set({ replyBody: body, replyIsUneditedAiDraft: true }),
  clearReply: () => set({ replyBody: '', replyIsUneditedAiDraft: false }),

  toast: null,
  showToast: (message) => set({ toast: message }),
  dismissToast: () => set({ toast: null }),
}));
