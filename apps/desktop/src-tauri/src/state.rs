//! Tauri の `app.manage()` に載せるアプリ状態。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mailstore::{paths, Store};

use crate::error::AppError;

/// 直近の同期の結果。UI のサイドバーに出す。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LastSync {
    /// RFC 3339。
    pub finished_at: String,
    pub inserted: usize,
    pub errors: usize,
    /// 失敗したときだけ。UI に出す日本語メッセージ（秘密情報を含めない）。
    pub error: Option<String>,
}

pub struct AppState {
    /// UI からの読み書き用。同期タスクは別コネクションを開くのでここは共有しない。
    store: Mutex<Store>,
    /// DB・raw .eml・添付の親ディレクトリ。
    pub data_dir: PathBuf,
    /// 同期実行中のアカウント。多重実行を拒否するために持つ。
    /// spawn したバックグラウンドタスクに持ち込むため `Arc` で共有する。
    pub syncing: Arc<Mutex<HashSet<i64>>>,
    /// 直近の同期結果。アカウント id → 結果。プロセスを再起動すると消える。
    /// TODO(P3-b): meta テーブルに永続化して再起動後も「N 分前に同期」を出す。
    pub last_sync: Arc<Mutex<HashMap<i64, LastSync>>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        paths::ensure_data_dir(&data_dir)?;
        let store = Store::open(paths::db_path(&data_dir))?;
        Ok(Self {
            store: Mutex::new(store),
            data_dir,
            syncing: Arc::new(Mutex::new(HashSet::new())),
            last_sync: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>, AppError> {
        self.store
            .lock()
            .map_err(|_| AppError::internal("内部状態のロックに失敗しました。"))
    }

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
