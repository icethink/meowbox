import type { ReactNode } from 'react';

/**
 * 3 ペイン + 右パネル。幅の配分はデザインどおり。
 *   sidebar 220 固定 / list 390 固定 / thread 伸縮 / panel 300（トグル）
 *
 * 広げたときに伸びるのはスレッド表示だけ。狭めたときは
 * 右パネル → サイドバー の順に畳む（CSS 側で hidden にする）。
 */
export function AppShell({
  sidebar,
  list,
  thread,
  panel,
}: {
  sidebar: ReactNode;
  list: ReactNode;
  thread: ReactNode;
  panel: ReactNode | null;
}) {
  return (
    <div className="h-full bg-app">
      <div className="flex h-full overflow-hidden rounded-lg border border-line bg-surface text-primary">
        {/* 900px を切ったらサイドバーを畳む */}
        <div className="contents max-[900px]:hidden">{sidebar}</div>
        {list}
        {thread}
        {/* 1200px を切ったら右パネルを畳む */}
        {panel && <div className="contents max-[1200px]:hidden">{panel}</div>}
      </div>
    </div>
  );
}
