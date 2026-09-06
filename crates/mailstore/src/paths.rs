//! データの置き場所。CLI とデスクトップアプリで同じ場所を指すよう、ここ 1 箇所で決める。
//!
//! 既定は OS のアプリデータディレクトリ配下:
//!   Windows: %APPDATA%\dev.icethink.meowbox\
//!   macOS:   ~/Library/Application Support/dev.icethink.meowbox/
//!   Linux:   ~/.local/share/dev.icethink.meowbox/
//! これは Tauri の `app_data_dir()`（identifier = dev.icethink.meowbox）と同じ場所になる。
//! 開発中は環境変数 MEOWBOX_DATA_DIR で丸ごと上書きできる。

use std::path::{Path, PathBuf};

use anyhow::Result;

/// apps/desktop/src-tauri/tauri.conf.json の identifier と同じ値。
/// 変えるときは両方を直すこと（Tauri の app_data_dir がこの名前でディレクトリを作る）。
pub const APP_DIR_NAME: &str = "dev.icethink.meowbox";

/// 環境変数 MEOWBOX_DATA_DIR の名前。
pub const DATA_DIR_ENV: &str = "MEOWBOX_DATA_DIR";

/// 既定のデータディレクトリ。MEOWBOX_DATA_DIR があればそれ、無ければ
/// OS のアプリデータディレクトリ配下の APP_DIR_NAME。
pub fn default_data_dir() -> Result<PathBuf> {
    data_dir_from(
        std::env::var(DATA_DIR_ENV).ok().as_deref(),
        dirs::data_dir(),
    )
}

/// 明示指定（CLI の --data-dir など）があればそれを優先し、無ければ `default_data_dir()`。
pub fn resolve_data_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    match explicit {
        Some(p) => Ok(p),
        None => default_data_dir(),
    }
}

/// SQLite ファイル: <data_dir>/meowbox.db
pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("meowbox.db")
}

/// raw .eml の置き場: <data_dir>/mail
pub fn mail_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("mail")
}

/// 展開した添付の置き場: <data_dir>/attachments
pub fn attachments_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("attachments")
}

/// data_dir と配下の mail / attachments を作る（既にあれば何もしない）。
pub fn ensure_data_dir(data_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::create_dir_all(mail_dir(data_dir))?;
    std::fs::create_dir_all(attachments_dir(data_dir))?;
    Ok(())
}

/// 純粋関数。env と base を引数で受け取るのでテストできる。
/// `env` が Some かつ空でなければそれを使う。無ければ `base`（= dirs::data_dir() の結果）に
/// APP_DIR_NAME を足す。base が None なら「アプリデータディレクトリを特定できない」エラー。
fn data_dir_from(env: Option<&str>, base: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(env) = env {
        if !env.is_empty() {
            return Ok(PathBuf::from(env));
        }
    }
    base.map(|b| b.join(APP_DIR_NAME))
        .ok_or_else(|| anyhow::anyhow!("could not determine the OS app data directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_takes_priority_over_base() {
        let dir = data_dir_from(Some("/custom/dir"), Some(PathBuf::from("/base"))).unwrap();
        assert_eq!(dir, PathBuf::from("/custom/dir"));
    }

    #[test]
    fn none_env_appends_app_dir_name_to_base() {
        let dir = data_dir_from(None, Some(PathBuf::from("/base"))).unwrap();
        assert_eq!(dir, PathBuf::from("/base").join(APP_DIR_NAME));
    }

    #[test]
    fn empty_env_falls_back_to_base() {
        let dir = data_dir_from(Some(""), Some(PathBuf::from("/base"))).unwrap();
        assert_eq!(dir, PathBuf::from("/base").join(APP_DIR_NAME));
    }

    #[test]
    fn no_base_and_no_env_is_error() {
        assert!(data_dir_from(None, None).is_err());
    }

    #[test]
    fn db_path_appends_filename() {
        let dir = PathBuf::from("/data");
        assert_eq!(db_path(&dir), PathBuf::from("/data/meowbox.db"));
    }

    #[test]
    fn mail_dir_appends_mail() {
        let dir = PathBuf::from("/data");
        assert_eq!(mail_dir(&dir), PathBuf::from("/data/mail"));
    }

    #[test]
    fn attachments_dir_appends_attachments() {
        let dir = PathBuf::from("/data");
        assert_eq!(attachments_dir(&dir), PathBuf::from("/data/attachments"));
    }
}
