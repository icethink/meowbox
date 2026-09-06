//! `meowbox-mcp` — Meowbox の MCP サーバ本体。stdio で待ち受ける独立バイナリ。
//!
//! GUI（Tauri アプリ）が起動していなくても Claude から使えるようにするため、
//! `mailstore::paths` で GUI・CLI と同じ DB / データディレクトリを開くだけの
//! 独立プロセスにしている。
//!
//! **stdout は JSON-RPC 専用。** MCP の stdio トランスポートは 1 行 = 1 メッセージの
//! JSON-RPC を stdout でやり取りする契約なので、ログ等が 1 行でも混ざると Claude 側の
//! パーサが壊れる。`println!` は使わず、ログは必ず stderr に出す。
//!
//! この単位（3）では `list_accounts` / `list_projects` の 2 ツールだけを実装する。
//! **`mark`（既読・アーカイブ）と送信は MCP に出さない**（ADR 0002 / 0007）。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use mailstore::Store;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Json;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt};

/// `list_accounts` の 1 要素。
///
/// **秘密情報は絶対に含めない**: `settings_json`（host / port / username を含む）は
/// ここに乗せない。ADR 0006 の「パスワードや接続設定を MCP に出さない」方針。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct AccountInfo {
    id: i64,
    name: String,
    email: String,
    project_tag: Option<String>,
    /// "imap" | "gmail" | "m365"
    kind: String,
    /// RFC 3339。まだ同期していなければ null。
    last_synced_at: Option<String>,
    unread_count: i64,
}

/// `list_projects` の中に出てくるアカウント。email 以外は載せない（軽量ビュー）。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct ProjectAccountInfo {
    id: i64,
    email: String,
}

/// `list_projects` の 1 グループ。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct ProjectInfo {
    /// project_tag。未設定のアカウントは null のグループにまとめ、一覧の最後に置く。
    project_tag: Option<String>,
    accounts: Vec<ProjectAccountInfo>,
    unread_count: i64,
}

/// すべてのツール description に必ず入れる、送信・既読変更をしないことの明記。
/// Claude が「このツールで送信できる／既読にできる」と誤解しないようにするための文言。
const SAFETY_NOTE_JA: &str = "送信はできません。既読状態は変更されません。";
const SAFETY_NOTE_EN: &str = "This server cannot send mail and never changes read state.";

struct MeowboxMcp {
    store: Mutex<Store>,
    tool_router: ToolRouter<Self>,
}

impl MeowboxMcp {
    fn new(store: Store) -> Self {
        Self {
            store: Mutex::new(store),
            tool_router: Self::tool_router(),
        }
    }

    /// `Store` は `Sync` ではないので `Mutex` に入れて共有している。
    /// poisoned（他のツール呼び出しが panic した）ときは `unwrap()` せず文字列エラーにする。
    fn lock_store(&self) -> Result<MutexGuard<'_, Store>, String> {
        self.store.lock().map_err(|_| {
            "内部状態のロックに失敗しました（他の呼び出しで異常終了した可能性があります）"
                .to_string()
        })
    }
}

#[tool_router]
impl MeowboxMcp {
    #[tool(
        description = "登録されているメールアカウントの一覧を返す。送信はできません。既読状態は変更されません。\n\
        List the configured mail accounts. This server cannot send mail and never changes read state."
    )]
    async fn list_accounts(&self) -> Result<Json<Vec<AccountInfo>>, String> {
        let store = self.lock_store()?;
        let accounts = store.list_accounts().map_err(|e| e.to_string())?;
        let unread: HashMap<i64, i64> = store
            .unread_counts_by_account()
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();

        let mut out = Vec::with_capacity(accounts.len());
        for a in accounts {
            let last_synced_at = store
                .account_synced_at(a.id)
                .map_err(|e| e.to_string())?
                .map(|dt| dt.to_rfc3339());
            out.push(AccountInfo {
                id: a.id,
                name: a.name,
                email: a.email,
                project_tag: a.project_tag,
                kind: a.kind.as_str().to_string(),
                last_synced_at,
                unread_count: unread.get(&a.id).copied().unwrap_or(0),
            });
        }
        Ok(Json(out))
    }

    #[tool(
        description = "案件（project_tag）ごとにアカウントをまとめた一覧を返す。\
        project_tag が無いアカウントは最後のグループにまとめる。送信はできません。既読状態は変更されません。\n\
        List accounts grouped by project tag; accounts without a tag are grouped last. \
        This server cannot send mail and never changes read state."
    )]
    async fn list_projects(&self) -> Result<Json<Vec<ProjectInfo>>, String> {
        let store = self.lock_store()?;
        let accounts = store.list_accounts().map_err(|e| e.to_string())?;
        let unread: HashMap<i64, i64> = store
            .unread_counts_by_account()
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();

        Ok(Json(group_accounts_by_project(&accounts, &unread)))
    }
}

/// `list_projects` の中身。`project_tag` でまとめ、タグが無いアカウントは
/// 最後のグループ（`project_tag: None`）にまとめる。純粋関数なのでテストしやすい。
fn group_accounts_by_project(
    accounts: &[mailcore::Account],
    unread: &HashMap<i64, i64>,
) -> Vec<ProjectInfo> {
    let mut tagged: BTreeMap<String, Vec<ProjectAccountInfo>> = BTreeMap::new();
    let mut untagged: Vec<ProjectAccountInfo> = Vec::new();
    for a in accounts {
        let pa = ProjectAccountInfo {
            id: a.id,
            email: a.email.clone(),
        };
        match &a.project_tag {
            Some(tag) => tagged.entry(tag.clone()).or_default().push(pa),
            None => untagged.push(pa),
        }
    }

    let sum_unread = |accs: &[ProjectAccountInfo]| -> i64 {
        accs.iter()
            .map(|a| unread.get(&a.id).copied().unwrap_or(0))
            .sum()
    };

    let mut groups: Vec<ProjectInfo> = tagged
        .into_iter()
        .map(|(tag, accounts)| {
            let unread_count = sum_unread(&accounts);
            ProjectInfo {
                project_tag: Some(tag),
                accounts,
                unread_count,
            }
        })
        .collect();

    if !untagged.is_empty() {
        let unread_count = sum_unread(&untagged);
        groups.push(ProjectInfo {
            project_tag: None,
            accounts: untagged,
            unread_count,
        });
    }

    groups
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MeowboxMcp {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        // rmcp::model::ServerInfo::new() 自身の既定値だと serverInfo.name が
        // （rmcp クレートをビルドしたときの）"rmcp" になってしまうため、明示的に上書きする。
        .with_server_info(rmcp::model::Implementation::new(
            "meowbox-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(format!(
            "Meowbox のメールデータを読み取り専用で公開する MCP サーバです。{SAFETY_NOTE_JA}\n\
             This server exposes Meowbox mail data read-only. {SAFETY_NOTE_EN}"
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // stdout は JSON-RPC 専用なので、ログは必ず stderr に出す（with_writer）。
    // println! も使わない。1 行でも stdout に混ざると Claude 側のプロトコルが壊れる。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = mailstore::paths::default_data_dir().context("resolve data dir")?;
    mailstore::paths::ensure_data_dir(&data_dir).context("ensure data dir")?;
    let db_path = mailstore::paths::db_path(&data_dir);
    // Store::open は無ければ作るので、空の DB に対しても list_accounts は空配列を返す。
    let store = Store::open(&db_path).with_context(|| format!("open {}", db_path.display()))?;

    let server = MeowboxMcp::new(store);
    server
        .serve(rmcp::transport::io::stdio())
        .await
        .context("start MCP stdio server")?
        .waiting()
        .await
        .context("MCP server loop")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::AccountKind;

    /// テスト用の `Account` を組み立てる。アドレスは RFC 2606 の `.example` のみ使う。
    fn account(id: i64, project_tag: Option<&str>) -> mailcore::Account {
        mailcore::Account {
            id,
            name: format!("account-{id}"),
            kind: AccountKind::Imap,
            email: format!("user{id}@mail.example"),
            project_tag: project_tag.map(str::to_string),
            settings: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn groups_accounts_by_project_tag_in_sorted_order() {
        let accounts = vec![
            account(1, Some("案件B")),
            account(2, Some("案件A")),
            account(3, Some("案件A")),
        ];
        let unread = HashMap::new();

        let groups = group_accounts_by_project(&accounts, &unread);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].project_tag.as_deref(), Some("案件A"));
        assert_eq!(groups[0].accounts.len(), 2);
        assert_eq!(groups[1].project_tag.as_deref(), Some("案件B"));
        assert_eq!(groups[1].accounts.len(), 1);
    }

    #[test]
    fn untagged_accounts_are_grouped_last_with_null_tag() {
        let accounts = vec![account(1, None), account(2, Some("案件A"))];
        let unread = HashMap::new();

        let groups = group_accounts_by_project(&accounts, &unread);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].project_tag.as_deref(), Some("案件A"));
        assert_eq!(groups[1].project_tag, None);
        assert_eq!(groups[1].accounts[0].id, 1);
    }

    #[test]
    fn no_untagged_accounts_means_no_null_group() {
        let accounts = vec![account(1, Some("案件A"))];
        let unread = HashMap::new();

        let groups = group_accounts_by_project(&accounts, &unread);

        assert!(groups.iter().all(|g| g.project_tag.is_some()));
    }

    #[test]
    fn unread_count_is_summed_per_project() {
        let accounts = vec![account(1, Some("案件A")), account(2, Some("案件A"))];
        let unread = HashMap::from([(1, 3), (2, 5)]);

        let groups = group_accounts_by_project(&accounts, &unread);

        assert_eq!(groups[0].unread_count, 8);
    }
}
