import { useEffect, useRef } from 'react';

/** 連打対策: 前回の読み込みからこの時間内のフォーカスは無視する */
const REFRESH_THROTTLE_MS = 2000;

/**
 * ウィンドウがフォーカスを得たら再読込する。Claude が MCP 経由で
 * 要約やタスクを書き込んでも GUI は別プロセスで気づかないため。
 *
 * `refresh` は毎レンダー新しい関数になりうるので ref に入れて保持し、
 * `window` へのリスナ登録・解除は初回・アンマウント時だけにする。
 */
export function useRefreshOnFocus(refresh: () => void | Promise<void>, enabled = true): void {
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  const lastRunRef = useRef(0);

  useEffect(() => {
    if (!enabled) return;

    function onFocus() {
      const now = Date.now();
      if (now - lastRunRef.current < REFRESH_THROTTLE_MS) return;
      lastRunRef.current = now;
      void refreshRef.current();
    }

    window.addEventListener('focus', onFocus);
    return () => {
      window.removeEventListener('focus', onFocus);
    };
  }, [enabled]);
}

/**
 * `intervalMs` ごとに自動で読み直す。既定オフ（Claude が MCP 経由で書き込んだものを
 * 拾うための保険）。`enabled` が false のときは何もしない。
 */
export function useAutoRefresh(
  refresh: () => void | Promise<void>,
  intervalMs: number,
  enabled: boolean,
): void {
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    if (!enabled) return;

    const id = setInterval(() => {
      void refreshRef.current();
    }, intervalMs);
    return () => {
      clearInterval(id);
    };
  }, [enabled, intervalMs]);
}
