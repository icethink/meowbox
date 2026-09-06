// meowbox-mcp のビルド済みバイナリを、Tauri の externalBin が要求する
// ターゲットトリプル付きのファイル名でコピーするスクリプト。
// `cargo build` は `meowbox-mcp(.exe)` を吐くだけなので、tauri build の前に
// `meowbox-mcp-<triple>(.exe)` にリネームしてコピーする必要がある。
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, copyFileSync, chmodSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
// apps/desktop/scripts -> apps/desktop -> apps -> <repo root>
const repoRoot = join(scriptDir, '..', '..', '..');

function getTargetTriple() {
  const output = execFileSync('rustc', ['-vV'], { encoding: 'utf8' });
  const line = output.split(/\r?\n/).find((l) => l.startsWith('host:'));
  if (!line) {
    console.error('rustc -vV の出力から host: の行が見つかりませんでした。');
    process.exit(1);
  }
  return line.slice('host:'.length).trim();
}

const exeSuffix = process.platform === 'win32' ? '.exe' : '';
const binName = `meowbox-mcp${exeSuffix}`;

const candidates = [
  join(repoRoot, 'target', 'release', binName),
  join(repoRoot, 'target', 'debug', binName),
];

const sourcePath = candidates.find((p) => existsSync(p));
if (!sourcePath) {
  console.error(
    'meowbox-mcp のビルド済みバイナリが見つかりません。先に `cargo build --release -p mailmcp` を実行してください。',
  );
  process.exit(1);
}

const triple = getTargetTriple();
const destDir = join(repoRoot, 'apps', 'desktop', 'src-tauri', 'binaries');
if (!existsSync(destDir)) {
  mkdirSync(destDir, { recursive: true });
}
const destPath = join(destDir, `meowbox-mcp-${triple}${exeSuffix}`);

copyFileSync(sourcePath, destPath);
// copyFileSync はプラットフォームの copyfile 実装次第でコピー元のパーミッション
// を引き継がないことがある（例: Linux で umask により実行ビットが落ちる）。
// Windows には実行ビットの概念が無いため、非 Windows のときだけ明示的に
// 実行可能にしておく。
if (process.platform !== 'win32') {
  chmodSync(destPath, 0o755);
}
console.log(`copied ${relative(repoRoot, sourcePath)} -> ${relative(repoRoot, destPath)}`);
