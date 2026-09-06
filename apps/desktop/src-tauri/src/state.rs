//! Tauri の `app.manage()` に載せるアプリ状態。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use mailstore::{paths, Store};

use crate::error::AppError;

pub struct AppState {
    /// UI からの読み書き用。同期タスクは別コネクションを開くのでここは共有しない。
    store: Mutex<Store>,
    /// DB・raw .eml・添付の親ディレクトリ。
    pub data_dir: PathBuf,
    /// 同期実行中のアカウント。多重実行を拒否するために持つ（単位 6 で使う）。
    /// このコミットではまだ読み書きするコマンドが無いので dead_code を許可する。
    #[allow(dead_code)]
    pub syncing: Mutex<HashSet<i64>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        paths::ensure_data_dir(&data_dir)?;
        let store = Store::open(paths::db_path(&data_dir))?;
        Ok(Self {
            store: Mutex::new(store),
            data_dir,
            syncing: Mutex::new(HashSet::new()),
        })
    }

    pub fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>, AppError> {
        self.store
            .lock()
            .map_err(|_| AppError::internal("内部状態のロックに失敗しました。"))
    }

    // このコミットではまだ呼び出すコマンドが無いものがあるので dead_code を許可する。
    #[allow(dead_code)]
    pub fn db_path(&self) -> PathBuf {
        paths::db_path(&self.data_dir)
    }

    pub fn mail_dir(&self) -> PathBuf {
        paths::mail_dir(&self.data_dir)
    }

    #[allow(dead_code)]
    pub fn attachments_dir(&self) -> PathBuf {
        paths::attachments_dir(&self.data_dir)
    }
}
