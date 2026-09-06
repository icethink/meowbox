//! `#[tauri::command]` の置き場。ロジックは `&Store` を取る素の関数に書き、
//! コマンド本体はそれを呼ぶだけの薄いラッパにする（テストしやすくするため）。

pub mod accounts;
pub mod sync;
pub mod threads;
