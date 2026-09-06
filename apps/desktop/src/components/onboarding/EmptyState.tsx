import { Logo } from '../sidebar/Logo';

/** 猫の線画。自作の単純な線画（丸い顔・三角の耳・ひげ数本）。外部アイコンは使わない */
function CatOutline() {
  return (
    <svg
      width="56"
      height="56"
      viewBox="0 0 56 56"
      fill="none"
      aria-hidden="true"
      className="text-faint"
    >
      {/* 顔 */}
      <circle cx="28" cy="32" r="16" stroke="currentColor" strokeWidth="1.6" />
      {/* 耳 */}
      <path
        d="M16 20 L14 8 L24 17"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M40 20 L42 8 L32 17"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      {/* 目 */}
      <circle cx="22" cy="30" r="1.4" fill="currentColor" />
      <circle cx="34" cy="30" r="1.4" fill="currentColor" />
      {/* ひげ 左 */}
      <path d="M14 33 L22 33" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      <path d="M14 37 L22 35" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      {/* ひげ 右 */}
      <path d="M42 33 L34 33" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      <path d="M42 37 L34 35" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

/** アカウントが 0 件のときに 3 ペインの代わりに全面表示する初回起動画面 */
export function EmptyState({ onAddAccount }: { onAddAccount: () => void }) {
  return (
    <div className="grid h-full place-items-center bg-app">
      <div className="flex max-w-[360px] flex-col items-center gap-[16px] text-center">
        <CatOutline />
        <div className="flex items-center gap-[8px]">
          <Logo size={22} />
          <span className="text-xl font-bold text-primary">Meowbox</span>
        </div>
        <p className="text-base leading-relaxed text-muted">
          案件ごとのメールアドレスをまとめて、Claude が読める形にします。
        </p>
        <button
          type="button"
          onClick={onAddAccount}
          className="mt-[4px] rounded-token bg-accent px-[18px] py-[8px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover"
        >
          アカウントを追加
        </button>
      </div>
    </div>
  );
}
