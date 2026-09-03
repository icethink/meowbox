//! Meowbox デスクトップシェル。
//!
//! P2 の時点では WebView を出すだけで、データはフロント側の `src/api/` が
//! モックを返している。P3 でそこを `invoke` に差し替え、ここに
//! `#[tauri::command]` を生やして mailstore を読む。

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running meowbox");
}
