//! Claude Desktop / Claude Code に `meowbox-mcp` を登録するための情報。
//!
//! **設定ファイルはアプリが勝手に書き換えない**（他人の設定を壊さない）。
//! ここではパスを解決し、コピペするだけで使える文字列を組み立てて返すだけにする。

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Claude に登録するための情報。パスとコピペ用の文字列だけを返す。
#[derive(Debug, Clone, serde::Serialize)]
pub struct McpIntegration {
    /// meowbox-mcp の絶対パス。
    pub server_path: String,
    /// そのパスに実行ファイルが実在するか。false ならまだビルドされていない（開発中など）。
    pub server_exists: bool,
    /// claude_desktop_config.json に貼る JSON。
    pub desktop_config_json: String,
    /// Claude Code / Cowork 用のコマンド 1 行。
    pub claude_code_command: String,
}

/// meowbox-mcp の実行ファイル名。Windows 以外は拡張子なし。
fn mcp_binary_name() -> &'static str {
    if cfg!(windows) {
        "meowbox-mcp.exe"
    } else {
        "meowbox-mcp"
    }
}

/// `current_exe` から上のディレクトリを辿って `target` を見つけ、
/// `target/release/<bin>` と `target/debug/<bin>` を候補として返す。
/// 開発中（`cargo tauri dev`）にビルド済みの `meowbox-mcp` を見つけるためのもの。
/// 深く作り込まない: 見つからなければ空の `Vec` を返す。
fn dev_build_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let bin = mcp_binary_name();
    let mut dir = current_exe.parent();
    while let Some(d) = dir {
        let target = d.join("target");
        if target.is_dir() {
            return vec![
                target.join("release").join(bin),
                target.join("debug").join(bin),
            ];
        }
        dir = d.parent();
    }
    Vec::new()
}

/// meowbox-mcp の実行ファイルパスを解決する。
///
/// 1. 基本: 自分の実行ファイルと同じディレクトリ（インストール後は `externalBin` で
///    ここに同梱される）。
/// 2. 開発中でそこに無ければ、リポジトリの `target/release` / `target/debug` を探す。
///
/// どこにも見つからなければ「基本のパス」を `exists = false` で返す。
fn resolve_server_path() -> Result<(PathBuf, bool), AppError> {
    let current_exe = std::env::current_exe().map_err(|e| {
        AppError::internal(format!("実行ファイルのパスが取得できませんでした: {e}"))
    })?;

    let bin = mcp_binary_name();
    let beside = current_exe
        .parent()
        .map(|dir| dir.join(bin))
        .unwrap_or_else(|| PathBuf::from(bin));

    if beside.is_file() {
        return Ok((beside, true));
    }

    for candidate in dev_build_candidates(&current_exe) {
        if candidate.is_file() {
            return Ok((candidate, true));
        }
    }

    Ok((beside, false))
}

/// `desktop_config_json` / `claude_code_command` の組み立て本体。
/// `current_exe()` に依存しないので純粋にテストできる。
fn build_integration(server_path: &Path, exists: bool) -> McpIntegration {
    let path_str = server_path.display().to_string();

    // Windows のパスはバックスラッシュを含むので、手で文字列連結せず
    // serde_json で組み立ててからシリアライズする（エスケープ漏れの防止）。
    let config = serde_json::json!({
        "mcpServers": {
            "meowbox": {
                "command": path_str,
                "args": [],
            }
        }
    });
    let desktop_config_json = serde_json::to_string_pretty(&config).unwrap_or_default();

    let claude_code_command = format!("claude mcp add meowbox -- \"{path_str}\"");

    McpIntegration {
        server_path: path_str,
        server_exists: exists,
        desktop_config_json,
        claude_code_command,
    }
}

#[tauri::command]
pub fn mcp_integration() -> Result<McpIntegration, AppError> {
    let (server_path, exists) = resolve_server_path()?;
    Ok(build_integration(&server_path, exists))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_config_json_escapes_windows_paths() {
        let path = Path::new(r"C:\x\meowbox-mcp.exe");
        let integration = build_integration(path, true);

        let value: serde_json::Value =
            serde_json::from_str(&integration.desktop_config_json).expect("valid json");
        let command = value["mcpServers"]["meowbox"]["command"]
            .as_str()
            .expect("command is a string");
        assert_eq!(command, r"C:\x\meowbox-mcp.exe");
    }

    #[test]
    fn claude_code_command_quotes_the_path() {
        let path = Path::new(r"C:\Program Files\Meowbox\meowbox-mcp.exe");
        let integration = build_integration(path, true);

        assert!(integration
            .claude_code_command
            .contains(r#""C:\Program Files\Meowbox\meowbox-mcp.exe""#));
    }
}
