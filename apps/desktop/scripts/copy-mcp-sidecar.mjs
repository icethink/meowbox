// meowbox-mcp のビルド済みバイナリを、Tauri の externalBin が要求する
// ターゲットトリプル付きのファイル名でコピーするスクリプト。
// `cargo build` は `meowbox-mcp(.exe)` を吐くだけなので、tauri build の前に
// `meowbox-mcp-<triple>(.exe)` にリネームしてコピーする必要がある。
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, copyFileSync, chmodSync } from 'node:fs';
import { dirname, isAbsolute, join, relative } from 'node:path';
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

// `--target <triple>` / `--target=<triple>` を読む。指定が無ければ undefined。
export function parseTargetArg(argv) {
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--target') {
      return argv[i + 1];
    }
    if (arg.startsWith('--target=')) {
      return arg.slice('--target='.length);
    }
  }
  return undefined;
}

// パス解決だけを行う純粋関数。ファイルシステム・rustc の実行・process.exit はここに置かない。
//
// - `explicitTarget` が true のとき（`--target` 指定あり）:
//   候補は `<targetDir>/<triple>/release` → `<targetDir>/<triple>/debug`（cargo が
//   `--target` 付きでビルドするとトリプル名のサブディレクトリに吐くため）で、
//   拡張子は `triple` に `windows` を含むかどうかで決める。
// - `explicitTarget` が false のとき（省略時、今までどおりの挙動）:
//   候補は `<targetDir>/release` → `<targetDir>/debug` で、拡張子は
//   `platform === 'win32'` で決める。
//
// どちらの場合も、コピー先のファイル名は解決済みの `triple`
// （明示指定 or `rustc -vV` の host）を使う。
export function resolveSidecarPaths({ repoRoot, cargoTargetDir, triple, explicitTarget, platform }) {
  const targetDir = cargoTargetDir
    ? isAbsolute(cargoTargetDir)
      ? cargoTargetDir
      : join(repoRoot, cargoTargetDir)
    : join(repoRoot, 'target');

  const exeSuffix = explicitTarget
    ? triple.includes('windows')
      ? '.exe'
      : ''
    : platform === 'win32'
      ? '.exe'
      : '';

  const binName = `meowbox-mcp${exeSuffix}`;

  const candidates = explicitTarget
    ? [join(targetDir, triple, 'release', binName), join(targetDir, triple, 'debug', binName)]
    : [join(targetDir, 'release', binName), join(targetDir, 'debug', binName)];

  const destDir = join(repoRoot, 'apps', 'desktop', 'src-tauri', 'binaries');
  const destPath = join(destDir, `meowbox-mcp-${triple}${exeSuffix}`);

  return { targetDir, candidates, exeSuffix, destDir, destPath };
}

function main() {
  const explicitTriple = parseTargetArg(process.argv.slice(2));
  const triple = explicitTriple ?? getTargetTriple();

  const { candidates, destDir, destPath } = resolveSidecarPaths({
    repoRoot,
    cargoTargetDir: process.env.CARGO_TARGET_DIR,
    triple,
    explicitTarget: explicitTriple !== undefined,
    platform: process.platform,
  });

  const sourcePath = candidates.find((p) => existsSync(p));
  if (!sourcePath) {
    console.error(
      'meowbox-mcp のビルド済みバイナリが見つかりません。先に `cargo build --release -p mailmcp` を実行してください。\n' +
        '探した候補:\n' +
        candidates.map((p) => `  - ${p}`).join('\n'),
    );
    process.exit(1);
  }

  if (!existsSync(destDir)) {
    mkdirSync(destDir, { recursive: true });
  }

  copyFileSync(sourcePath, destPath);
  // copyFileSync はプラットフォームの copyfile 実装次第でコピー元のパーミッション
  // を引き継がないことがある（例: Linux で umask により実行ビットが落ちる）。
  // Windows には実行ビットの概念が無いため、非 Windows のときだけ明示的に
  // 実行可能にしておく。
  if (process.platform !== 'win32') {
    chmodSync(destPath, 0o755);
  }
  console.log(`copied ${relative(repoRoot, sourcePath)} -> ${relative(repoRoot, destPath)}`);
}

// vitest からのテスト用 import 時は実行しない。
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
