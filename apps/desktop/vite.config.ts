import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// Tauri は固定ポートの dev server を前提にする（変わると WebView が繋がらない）。
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri CLI 側の出力を潰さない
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  // WebView2 は常に新しいので、ダウンレベルの変換は不要
  build: {
    target: 'chrome105',
    minify: 'esbuild',
    sourcemap: false,
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./vitest.setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    css: false,
    // vitest では Tauri の invoke が使えないため、api/index.ts に常にモック実装を
    // 選ばせる。実行環境にかかわらずテストが Tauri プロセス無しで完結するようにする。
    env: { VITE_MEOWBOX_MOCK: '1' },
  },
});
