//! アカウントの同期コマンド。`sync_account` はバックグラウンドタスクを起こしてすぐ返り、
//! 進捗は `sync://progress` イベントで逐次流す。UI は待たずにイベントを購読する。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use mailstore::Store;
use mailsync::engine::{SyncEngine, SyncOptions, SyncProgress};
use mailsync::imap::{ImapBackend, ImapConfig};
use tauri::Emitter;

use crate::error::AppError;
use crate::state::{AppState, LastSync};

/// `sync://progress` で UI に流す進捗。
/// **件数とフォルダ名だけ**。本文・アドレス・パスワードは絶対に載せない。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncProgressEvent {
    pub account_id: i64,
    /// いま処理しているフォルダ。完了通知では空文字列。
    pub folder: String,
    pub fetched: usize,
    /// `fetched` の分母。
    pub total: usize,
    pub inserted: usize,
    pub errors: usize,
    /// アカウント 1 回分の同期が終わったら true。
    pub done: bool,
    /// 同期が失敗したときだけ入る日本語メッセージ。`done = true` と一緒に来る。
    pub error: Option<String>,
}

/// イベント名。UI 側（`apps/desktop/src/api/`）と揃えること。
pub const SYNC_PROGRESS_EVENT: &str = "sync://progress";

/// 同期中フラグを立てる。既に走っていれば `conflict`。
/// 戻り値の `SyncGuard` を drop するとフラグが下りる（早期 return やパニックでも下りる）。
pub fn begin_sync(
    syncing: &Arc<Mutex<HashSet<i64>>>,
    account_id: i64,
) -> Result<SyncGuard, AppError> {
    let mut guard = syncing
        .lock()
        .map_err(|_| AppError::internal("内部状態のロックに失敗しました。"))?;
    if !guard.insert(account_id) {
        return Err(AppError::conflict("このアカウントは同期中です"));
    }
    drop(guard);
    Ok(SyncGuard {
        syncing: syncing.clone(),
        account_id,
    })
}

/// drop でフラグを下ろす。
#[derive(Debug)]
pub struct SyncGuard {
    syncing: Arc<Mutex<HashSet<i64>>>,
    account_id: i64,
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        // ロックが poisoned な場合、Drop はエラーを返せないので何もしない。
        // フラグが下りないままになるが、他のスレッドが panic した後の状態なので
        // プロセス自体の健全性がすでに疑わしく、ここで無理に握り直す価値は薄い。
        if let Ok(mut guard) = self.syncing.lock() {
            guard.remove(&self.account_id);
        }
    }
}

/// mailsync からのエラーを UI 向けの日本語メッセージに変換する。
/// パスワードや資格情報が混ざる経路を作らないため、`BackendError` を取り出せたときだけ
/// `AppError::from_backend` の考え方に沿ったメッセージにし、それ以外は固定文言にする。
fn sync_error_message(err: &anyhow::Error) -> String {
    match err.downcast_ref::<mailcore::BackendError>() {
        Some(mailcore::BackendError::Auth(_)) => {
            "認証に失敗しました。ユーザー名とパスワードを確認してください。".to_string()
        }
        Some(mailcore::BackendError::Network(msg)) => format!("接続できませんでした: {msg}"),
        Some(mailcore::BackendError::Protocol(_)) | None => "同期に失敗しました".to_string(),
    }
}

#[tauri::command]
pub async fn sync_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), AppError> {
    let account = state
        .store()?
        .get_account(id)?
        .ok_or_else(|| AppError::not_found("アカウントが見つかりません"))?;

    let config = ImapConfig::from_account(&account).map_err(|e| match e {
        mailsync::imap::ConfigError::NotImap { .. } => {
            AppError::invalid_input("Gmail / Microsoft 365 はまだ対応していません")
        }
        mailsync::imap::ConfigError::NoHost => {
            AppError::invalid_input("サーバー（ホスト）が設定されていません")
        }
    })?;

    let guard = begin_sync(&state.syncing, id)?;

    let db_path = state.db_path();
    let mail_dir = state.mail_dir();
    let last_sync = state.last_sync.clone();

    // `mailstore::Store`（延いては `SyncEngine`）は内部に `RefCell` を持つため `Sync` ではなく、
    // `Arc<Store>` を `.await` をまたいで保持する future は `Send` にならない。
    // `tauri::async_runtime::spawn`（マルチスレッドの Tokio ランタイム）はタスクが
    // ワーカースレッド間を移動できることを前提に `Send` を要求するため、そのままでは
    // spawn できない。専用の OS スレッドを 1 本立て、その上にシングルスレッドの
    // Tokio ランタイムを作って同期処理を完結させることで回避する
    // （スレッドを跨いで「移動」しないので `Send` は不要）。
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let message = format!("同期タスクの実行環境を作成できませんでした: {e}");
                emit_done_error(&app, id, &message);
                record_error(&last_sync, id, message);
                drop(guard);
                return;
            }
        };
        runtime.block_on(run_sync(
            app, guard, db_path, mail_dir, last_sync, id, config,
        ));
    });

    Ok(())
}

/// バックグラウンドタスクの本体。同期タスクは UI 用の接続とは別に自分の接続を開く
/// （SQLite は WAL なので並行して読み書きできる）。
async fn run_sync(
    app: tauri::AppHandle,
    guard: SyncGuard,
    db_path: std::path::PathBuf,
    mail_dir: std::path::PathBuf,
    last_sync: Arc<Mutex<HashMap<i64, LastSync>>>,
    account_id: i64,
    config: ImapConfig,
) {
    let store = match Store::open(&db_path) {
        Ok(store) => store,
        Err(e) => {
            let message = format!("データベースを開けませんでした: {e}");
            emit_done_error(&app, account_id, &message);
            record_error(&last_sync, account_id, message);
            drop(guard);
            return;
        }
    };

    // SyncEngine::store は Arc<Store> 固定。このタスクは単一の同期を行うだけなので
    // 複数スレッドから共有されない false positive。
    #[allow(clippy::arc_with_non_send_sync)]
    let engine = SyncEngine::new(Arc::new(store));
    let backend = ImapBackend::new(config);

    let app_for_sink = app.clone();
    let sink: mailsync::engine::ProgressSink = Arc::new(move |p: SyncProgress| {
        // UI が既に閉じているだけのことがあるので、emit の失敗は無視する。
        let _ = app_for_sink.emit(
            SYNC_PROGRESS_EVENT,
            SyncProgressEvent {
                account_id,
                folder: p.folder,
                fetched: p.fetched,
                total: p.total,
                inserted: p.inserted,
                errors: p.errors,
                done: p.done,
                error: None,
            },
        );
    });

    let opts = SyncOptions {
        // P3-a では INBOX だけ同期する。Sent / Trash まで取ると一覧に自分の送信メールが
        // 混ざって初回の見え方が悪くなるため。TODO(P4): 他フォルダと IDLE。
        only_folder: Some("INBOX".to_string()),
        since: Some(Utc::now() - Duration::days(90)),
        data_dir: mail_dir,
        progress: Some(sink),
    };

    match engine.sync_once(account_id, &backend, &opts).await {
        Ok(report) => {
            record_success(&last_sync, account_id, report.inserted, report.errors);
            // meta テーブルへの永続化。別プロセス（MCP サーバ）が `last_synced_at` を
            // 読めるようにするためで、失敗しても取り込んだメールを無駄にしないよう
            // warn に残すだけで続行する。
            if let Err(e) = record_synced_at(&engine.store, account_id, Utc::now()) {
                tracing::warn!("最終同期時刻の記録に失敗しました: {e}");
            }
            // engine が done: true のイベントを出しているので、ここでは追加のイベントを出さない。
        }
        Err(e) => {
            let message = sync_error_message(&e);
            record_error(&last_sync, account_id, message.clone());
            // sync_once がエラーで返る経路では engine 側は done を出さないので、
            // ここで 1 回だけ出す。
            emit_done_error(&app, account_id, &message);
        }
    }

    drop(guard);
}

fn emit_done_error(app: &tauri::AppHandle, account_id: i64, message: &str) {
    let _ = app.emit(
        SYNC_PROGRESS_EVENT,
        SyncProgressEvent {
            account_id,
            folder: String::new(),
            fetched: 0,
            total: 0,
            inserted: 0,
            errors: 0,
            done: true,
            error: Some(message.to_string()),
        },
    );
}

/// `meta` テーブルに最終同期時刻を書くためのキー。
/// **この形式は MCP サーバ側（別プロセス）でも同じものを読むので、変えないこと。**
fn synced_at_key(account_id: i64) -> String {
    format!("sync:{account_id}:finished_at")
}

/// 同期成功時刻を `meta` テーブルに書く。ネットワーク・keyring を伴わない部分だけを
/// 切り出してあるので、テストではこの関数を直接呼ぶ。
fn record_synced_at(store: &Store, account_id: i64, at: DateTime<Utc>) -> anyhow::Result<()> {
    store.set_meta(&synced_at_key(account_id), &at.to_rfc3339())
}

fn record_success(
    last_sync: &Arc<Mutex<HashMap<i64, LastSync>>>,
    account_id: i64,
    inserted: usize,
    errors: usize,
) {
    if let Ok(mut map) = last_sync.lock() {
        map.insert(
            account_id,
            LastSync {
                finished_at: Utc::now().to_rfc3339(),
                inserted,
                errors,
                error: None,
            },
        );
    }
}

fn record_error(last_sync: &Arc<Mutex<HashMap<i64, LastSync>>>, account_id: i64, message: String) {
    if let Ok(mut map) = last_sync.lock() {
        map.insert(
            account_id,
            LastSync {
                finished_at: Utc::now().to_rfc3339(),
                inserted: 0,
                errors: 0,
                error: Some(message),
            },
        );
    }
}

/// アカウントごとの直近の同期結果。サイドバーの「N 分前に同期」に使う。
#[tauri::command]
pub fn sync_status(state: tauri::State<'_, AppState>) -> Result<HashMap<i64, LastSync>, AppError> {
    let map = state
        .last_sync
        .lock()
        .map_err(|_| AppError::internal("内部状態のロックに失敗しました。"))?;
    Ok(map.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ネットワークと keyring を触る sync_account の本体（IMAP 接続・DB オープン・
    // イベント送出）は CI で検証できないため、ここでは多重実行の防御ロジックと
    // イベントのフィールド構成だけをテストする。

    #[test]
    fn begin_sync_rejects_a_second_run_for_the_same_account() {
        let syncing = Arc::new(Mutex::new(HashSet::new()));
        let _guard = begin_sync(&syncing, 1).unwrap();
        let err = begin_sync(&syncing, 1).unwrap_err();
        assert_eq!(err.code, "conflict");
    }

    #[test]
    fn begin_sync_allows_a_different_account() {
        let syncing = Arc::new(Mutex::new(HashSet::new()));
        let _guard1 = begin_sync(&syncing, 1).unwrap();
        let _guard2 = begin_sync(&syncing, 2).unwrap();
    }

    #[test]
    fn dropping_the_guard_allows_the_next_run() {
        let syncing = Arc::new(Mutex::new(HashSet::new()));
        let guard = begin_sync(&syncing, 1).unwrap();
        drop(guard);
        let _guard2 = begin_sync(&syncing, 1).unwrap();
    }

    #[test]
    fn synced_at_key_has_the_shape_the_mcp_server_reads() {
        assert_eq!(synced_at_key(42), "sync:42:finished_at");
    }

    #[test]
    fn record_synced_at_can_be_read_back_via_get_meta() {
        let store = Store::open_in_memory().unwrap();
        let at = Utc::now();
        record_synced_at(&store, 42, at).unwrap();
        let value = store.get_meta(&synced_at_key(42)).unwrap();
        assert_eq!(value, Some(at.to_rfc3339()));
    }

    #[test]
    fn sync_progress_event_carries_no_message_content() {
        let event = SyncProgressEvent {
            account_id: 1,
            folder: "INBOX".to_string(),
            fetched: 1,
            total: 2,
            inserted: 1,
            errors: 0,
            done: false,
            error: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        let obj = json.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "account_id",
                "done",
                "error",
                "errors",
                "fetched",
                "folder",
                "inserted",
                "total",
            ]
        );
    }
}
