//! Meowbox デスクトップシェル。
//!
//! `AppState` を通じて `mailstore::Store` を保持し、UI からは
//! `#[tauri::command]`（`commands` モジュール）経由でだけ DB を触れるようにする。
//! DB・raw .eml・添付は OS のアプリデータディレクトリ配下（開発中は
//! `MEOWBOX_DATA_DIR` で上書き可能）に置き、CLI の `mailstore::paths::default_data_dir()`
//! と同じ場所を指す。

mod commands;
mod error;
mod state;

use std::path::PathBuf;

use tauri::Manager;

use state::AppState;

/// アプリのデータディレクトリを決める。`MEOWBOX_DATA_DIR`（`mailstore::paths::DATA_DIR_ENV`）
/// が空でなければそれを使い、無ければ Tauri の `app_data_dir()` を使う。
/// identifier（`tauri.conf.json` の `dev.icethink.meowbox` = `mailstore::paths::APP_DIR_NAME`）
/// が同じなので、CLI の `mailstore::paths::default_data_dir()` と同じ場所になる。
fn resolve_data_dir(app: &tauri::App) -> anyhow::Result<PathBuf> {
    match std::env::var(mailstore::paths::DATA_DIR_ENV) {
        Ok(v) if !v.is_empty() => Ok(PathBuf::from(v)),
        _ => Ok(app.path().app_data_dir()?),
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = resolve_data_dir(app)?;
            app.manage(AppState::new(data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::accounts::list_accounts,
            commands::accounts::add_account,
            commands::accounts::set_account_password,
            commands::accounts::test_connection,
            commands::accounts::delete_account,
            commands::accounts::list_projects,
            commands::sync::sync_account,
            commands::sync::sync_status,
            commands::threads::list_threads,
            commands::threads::get_thread,
            commands::threads::get_message,
            commands::threads::extract_attachment,
            commands::threads::open_attachment,
            commands::threads::get_digest,
            commands::threads::mark,
            commands::threads::view_counts,
            commands::drafts::create_draft,
            commands::drafts::list_drafts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running meowbox");
}
