/** 猫耳つきの封筒。色は currentColor 経由で --accent を受ける */
export function Logo({ size = 24 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className="shrink-0 text-accent"
    >
      <path d="M4 9 L4 5 L8 8 Z" fill="currentColor" />
      <path d="M20 9 L20 5 L16 8 Z" fill="currentColor" />
      <rect x="3" y="8" width="18" height="13" rx="3" stroke="currentColor" strokeWidth="1.6" />
      <path
        d="M4 10 L12 16 L20 10"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
