/**
 * データアクセス層。UI からはこの関数群だけを呼ぶ。
 *
 * モックを使うのは `VITE_MEOWBOX_MOCK=1` のときだけ（vitest と、Tauri 無しでの
 * 見た目確認用）。それ以外は Tauri の `invoke` を使う実データ実装（`./tauri`）。
 */

import { mockApi } from './mock';
import { tauriApi } from './tauri';

const useMock = import.meta.env.VITE_MEOWBOX_MOCK === '1';

const impl = useMock ? mockApi : tauriApi;

export const listAccounts = impl.listAccounts;
export const addAccount = impl.addAccount;
export const setAccountPassword = impl.setAccountPassword;
export const testConnection = impl.testConnection;
export const deleteAccount = impl.deleteAccount;

export const listProjects = impl.listProjects;
export const listViews = impl.listViews;

export const listThreads = impl.listThreads;
export const getThread = impl.getThread;
export const getMessage = impl.getMessage;
export const extractAttachment = impl.extractAttachment;
export const openAttachment = impl.openAttachment;

export const getDigest = impl.getDigest;
export const mark = impl.mark;

export const syncAccount = impl.syncAccount;
export const listSyncStatus = impl.listSyncStatus;
export const onSyncProgress = impl.onSyncProgress;
export const getSyncStatus = impl.getSyncStatus;

export const createDraft = impl.createDraft;
export const listDrafts = impl.listDrafts;
export const generateAiDraft = impl.generateAiDraft;
export const sendDraft = impl.sendDraft;

export type { MarkAction, MeowboxApi, ThreadQuery } from './contract';
