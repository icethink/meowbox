import { describe, expect, it } from 'vitest';
import { join } from 'node:path';
import { parseTargetArg, resolveSidecarPaths } from './copy-mcp-sidecar.mjs';

const repoRoot = join('C:', 'repo');

describe('parseTargetArg', () => {
  it('returns undefined when --target is absent', () => {
    expect(parseTargetArg([])).toBeUndefined();
  });

  it('reads --target <triple>', () => {
    expect(parseTargetArg(['--target', 'aarch64-apple-darwin'])).toBe('aarch64-apple-darwin');
  });

  it('reads --target=<triple>', () => {
    expect(parseTargetArg(['--target=x86_64-pc-windows-msvc'])).toBe('x86_64-pc-windows-msvc');
  });
});

describe('resolveSidecarPaths', () => {
  it('falls back to <repoRoot>/target/{release,debug} when neither --target nor CARGO_TARGET_DIR is set', () => {
    const result = resolveSidecarPaths({
      repoRoot,
      cargoTargetDir: undefined,
      triple: 'x86_64-pc-windows-msvc',
      explicitTarget: false,
      platform: 'win32',
    });

    expect(result.candidates).toEqual([
      join(repoRoot, 'target', 'release', 'meowbox-mcp.exe'),
      join(repoRoot, 'target', 'debug', 'meowbox-mcp.exe'),
    ]);
    expect(result.exeSuffix).toBe('.exe');
    expect(result.destPath).toBe(
      join(repoRoot, 'apps', 'desktop', 'src-tauri', 'binaries', 'meowbox-mcp-x86_64-pc-windows-msvc.exe'),
    );
  });

  it('uses CARGO_TARGET_DIR as the base target directory when set', () => {
    const cargoTargetDir = join('C:', 'ci-target');
    const result = resolveSidecarPaths({
      repoRoot,
      cargoTargetDir,
      triple: 'x86_64-unknown-linux-gnu',
      explicitTarget: false,
      platform: 'linux',
    });

    expect(result.targetDir).toBe(cargoTargetDir);
    expect(result.candidates).toEqual([
      join(cargoTargetDir, 'release', 'meowbox-mcp'),
      join(cargoTargetDir, 'debug', 'meowbox-mcp'),
    ]);
    expect(result.exeSuffix).toBe('');
  });

  it('resolves a relative CARGO_TARGET_DIR against repoRoot', () => {
    const result = resolveSidecarPaths({
      repoRoot,
      cargoTargetDir: join('build', 'target'),
      triple: 'x86_64-unknown-linux-gnu',
      explicitTarget: false,
      platform: 'linux',
    });

    expect(result.targetDir).toBe(join(repoRoot, 'build', 'target'));
  });

  it('inserts the triple subdirectory and uses an empty exe suffix for a non-windows --target', () => {
    const result = resolveSidecarPaths({
      repoRoot,
      cargoTargetDir: undefined,
      triple: 'aarch64-apple-darwin',
      explicitTarget: true,
      platform: 'win32', // わざと host と食い違わせて、triple 側の判定を使うことを確認する
    });

    expect(result.candidates).toEqual([
      join(repoRoot, 'target', 'aarch64-apple-darwin', 'release', 'meowbox-mcp'),
      join(repoRoot, 'target', 'aarch64-apple-darwin', 'debug', 'meowbox-mcp'),
    ]);
    expect(result.exeSuffix).toBe('');
    expect(result.destPath).toBe(
      join(repoRoot, 'apps', 'desktop', 'src-tauri', 'binaries', 'meowbox-mcp-aarch64-apple-darwin'),
    );
  });

  it('uses a .exe suffix for a windows --target', () => {
    const result = resolveSidecarPaths({
      repoRoot,
      cargoTargetDir: undefined,
      triple: 'x86_64-pc-windows-msvc',
      explicitTarget: true,
      platform: 'linux', // わざと host と食い違わせて、triple 側の判定を使うことを確認する
    });

    expect(result.candidates).toEqual([
      join(repoRoot, 'target', 'x86_64-pc-windows-msvc', 'release', 'meowbox-mcp.exe'),
      join(repoRoot, 'target', 'x86_64-pc-windows-msvc', 'debug', 'meowbox-mcp.exe'),
    ]);
    expect(result.exeSuffix).toBe('.exe');
    expect(result.destPath).toBe(
      join(repoRoot, 'apps', 'desktop', 'src-tauri', 'binaries', 'meowbox-mcp-x86_64-pc-windows-msvc.exe'),
    );
  });
});
