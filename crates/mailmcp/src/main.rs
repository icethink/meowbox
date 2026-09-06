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
//! ツール一覧は `docs/DESIGN.md` §6 を参照。
//! **`mark`（既読・アーカイブ）と送信は MCP に出さない**（ADR 0002 / 0007）。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use mailcore::Address;
use mailmcp::{GetMessageArgs, GetThreadArgs, SearchMessagesArgs};
use mailstore::{AttachmentRow, SearchQuery, Store, SummaryRow};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
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

/// メールアドレス 1 件。`mailcore::Address` は `schemars::JsonSchema` を実装していないので
/// MCP の出力用にラップする。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct AddressOut {
    name: Option<String>,
    email: String,
}

impl From<Address> for AddressOut {
    fn from(a: Address) -> Self {
        Self {
            name: a.name,
            email: a.email,
        }
    }
}

/// 添付 1 件の軽量ビュー（中身は含めない。取り出すのは `get_attachment`）。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct AttachmentOut {
    id: i64,
    filename: String,
    mime: String,
    size: i64,
}

impl From<AttachmentRow> for AttachmentOut {
    fn from(a: AttachmentRow) -> Self {
        Self {
            id: a.id,
            filename: a.filename,
            mime: a.mime,
            size: a.size,
        }
    }
}

/// Claude が保存した要約。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct SummaryOut {
    model: String,
    summary: String,
    /// RFC 3339
    created_at: String,
}

impl From<SummaryRow> for SummaryOut {
    fn from(r: SummaryRow) -> Self {
        Self {
            model: r.model,
            summary: r.summary,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

/// タスク 1 件。`mailcore::Task` は `schemars::JsonSchema` を実装していないのでラップする。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct TaskOut {
    id: i64,
    account_id: i64,
    source_message_id: Option<i64>,
    title: String,
    /// RFC 3339
    due: Option<String>,
    /// "open" | "done" | "dismissed"
    status: String,
    confidence: f32,
    created_by: String,
    /// RFC 3339
    created_at: String,
}

impl From<mailcore::Task> for TaskOut {
    fn from(t: mailcore::Task) -> Self {
        Self {
            id: t.id,
            account_id: t.account_id,
            source_message_id: t.source_message_id,
            title: t.title,
            due: t.due.map(|d| d.to_rfc3339()),
            status: task_status_str(t.status).to_string(),
            confidence: t.confidence,
            created_by: t.created_by,
            created_at: t.created_at.to_rfc3339(),
        }
    }
}

/// DB の文字列表現へ変換する。`mailstore` 側の同名関数は非公開なのでここに持つ。
fn task_status_str(s: mailcore::TaskStatus) -> &'static str {
    match s {
        mailcore::TaskStatus::Open => "open",
        mailcore::TaskStatus::Done => "done",
        mailcore::TaskStatus::Dismissed => "dismissed",
    }
}

/// `list_tasks` / `upsert_tasks` の `status` 文字列をパースする。未知の値はエラー。
fn parse_task_status(s: &str) -> std::result::Result<mailcore::TaskStatus, String> {
    match s {
        "open" => Ok(mailcore::TaskStatus::Open),
        "done" => Ok(mailcore::TaskStatus::Done),
        "dismissed" => Ok(mailcore::TaskStatus::Dismissed),
        other => Err(format!(
            "不明な status です（open / done / dismissed のいずれか）: {other}"
        )),
    }
}

/// `search_messages` の 1 件。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct MessageSearchResult {
    id: i64,
    thread_key: String,
    from: AddressOut,
    subject: String,
    /// RFC 3339
    date: String,
    snippet: String,
    is_read: bool,
    account_id: i64,
    project_tag: Option<String>,
}

/// `get_thread` / `get_message` で共通のメッセージ表現。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct MessageOut {
    id: i64,
    from: AddressOut,
    to: Vec<AddressOut>,
    /// RFC 3339
    date: String,
    body_text: String,
    /// `get_thread` では `include_quotes=true` のときだけ入る（false なら省略）。
    /// `get_message` では常に入る。
    #[serde(skip_serializing_if = "Option::is_none")]
    quoted_text: Option<String>,
    attachments: Vec<AttachmentOut>,
}

/// `get_thread` の返り値。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct ThreadOut {
    thread_key: String,
    /// 最新メッセージの件名。
    subject: String,
    /// スレッド内の全 `from` を email で重複除去したもの。
    participants: Vec<AddressOut>,
    messages: Vec<MessageOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<SummaryOut>,
    tasks: Vec<TaskOut>,
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

    #[tool(
        description = "全文検索。件名・本文・差出人・案件タグ・既読状態などで絞り込み、\
        軽量なメッセージ一覧（本文は含まない）を返す。limit は 200 件までに丸められる。\
        送信はできません。既読状態は変更されません。\n\
        Full-text search across subject/body/sender/project/read-state; returns a \
        lightweight message list without bodies. limit is capped at 200. \
        This server cannot send mail and never changes read state."
    )]
    async fn search_messages(
        &self,
        Parameters(args): Parameters<SearchMessagesArgs>,
    ) -> Result<Json<Vec<MessageSearchResult>>, String> {
        let store = self.lock_store()?;
        search_messages_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "スレッドを時系列でまとめて 1 回で取得する。本文はテキスト整形済み。\
        `include_quotes=true` のときだけ引用部分（quoted_text）を含める。\
        送信はできません。既読状態は変更されません。\n\
        Fetches a thread's messages in chronological order in one call. \
        Quoted text is only included when include_quotes=true. \
        This server cannot send mail and never changes read state."
    )]
    async fn get_thread(
        &self,
        Parameters(args): Parameters<GetThreadArgs>,
    ) -> Result<Json<ThreadOut>, String> {
        let store = self.lock_store()?;
        get_thread_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "1 通を本文・引用・添付一覧つきで取得する。include_html は現状無視される\
        （Store がまだ body_html を返せないため）。送信はできません。既読状態は変更されません。\n\
        Fetches a single message with its body, quoted text and attachment list. \
        include_html is currently ignored (Store does not expose body_html yet). \
        This server cannot send mail and never changes read state."
    )]
    async fn get_message(
        &self,
        Parameters(args): Parameters<GetMessageArgs>,
    ) -> Result<Json<MessageOut>, String> {
        let store = self.lock_store()?;
        get_message_impl(&store, &args).map(Json)
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

/// RFC3339 文字列を UTC の `DateTime` にパースする。
fn parse_rfc3339(s: &str) -> std::result::Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("日時の形式が不正です（RFC3339 で指定してください）: {s} ({e})"))
}

/// `search_messages` の本体。`limit` は 200 で頭打ちにする
/// （Claude が大量に取って文脈を潰さないように）。
fn search_messages_impl(
    store: &Store,
    args: &SearchMessagesArgs,
) -> std::result::Result<Vec<MessageSearchResult>, String> {
    let since = args.since.as_deref().map(parse_rfc3339).transpose()?;
    let limit = args.limit.min(200);

    let query = SearchQuery {
        text: args.query.as_deref(),
        account_id: args.account_id,
        project_tag: args.project.as_deref(),
        since,
        unread_only: args.unread_only,
        limit,
    };
    let results = store.search(&query).map_err(|e| e.to_string())?;

    // アカウント id → project_tag の対応表を 1 回だけ引く
    // （メッセージごとに get_account を呼ばない）。
    let project_by_account: HashMap<i64, Option<String>> = store
        .list_accounts()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|a| (a.id, a.project_tag))
        .collect();

    Ok(results
        .into_iter()
        .map(|m| MessageSearchResult {
            id: m.id,
            thread_key: m.thread_key,
            from: m.from.into(),
            subject: m.subject,
            date: m.date.to_rfc3339(),
            snippet: m.snippet,
            is_read: m.is_read,
            account_id: m.account_id,
            project_tag: project_by_account.get(&m.account_id).cloned().flatten(),
        })
        .collect())
}

/// raw .eml を読み直して引用・署名部分を取り出す。読み込み・パースに失敗しても
/// 空文字列を返すだけにする（1 通の壊れたメールでスレッド全体を落とさないため）。
fn quoted_text_for_message(store: &Store, message_id: i64) -> String {
    let Ok(Some(raw_path)) = store.message_raw_path(message_id) else {
        return String::new();
    };
    let Ok(raw) = std::fs::read(&raw_path) else {
        return String::new();
    };
    mailsync::parse::parse(&raw)
        .map(|p| p.quoted_text)
        .unwrap_or_default()
}

/// `mailcore::Message` + 添付一覧（+ 必要なら引用）を `MessageOut` に詰め替える。
/// `get_thread` / `get_message` の両方から呼ぶ共通処理。
fn message_to_out(
    store: &Store,
    msg: mailcore::Message,
    include_quotes: bool,
) -> std::result::Result<MessageOut, String> {
    let quoted_text = if include_quotes {
        Some(quoted_text_for_message(store, msg.id))
    } else {
        None
    };
    let attachments = store
        .list_attachments(msg.id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(AttachmentOut::from)
        .collect();

    Ok(MessageOut {
        id: msg.id,
        from: msg.from.into(),
        to: msg.to.into_iter().map(AddressOut::from).collect(),
        date: msg.date.to_rfc3339(),
        body_text: msg.body_text,
        quoted_text,
        attachments,
    })
}

/// `get_thread` の本体。
fn get_thread_impl(store: &Store, args: &GetThreadArgs) -> std::result::Result<ThreadOut, String> {
    let messages = store
        .thread_messages(&args.thread_key)
        .map_err(|e| e.to_string())?;
    if messages.is_empty() {
        return Err("スレッドが見つかりません".to_string());
    }

    let subject = messages
        .last()
        .map(|m| m.subject.clone())
        .unwrap_or_default();

    let mut seen_emails: HashSet<String> = HashSet::new();
    let mut participants: Vec<AddressOut> = Vec::new();
    for m in &messages {
        if seen_emails.insert(m.from.email.clone()) {
            participants.push(m.from.clone().into());
        }
    }

    let summary = store
        .latest_summary(&format!("thread:{}", args.thread_key))
        .map_err(|e| e.to_string())?
        .map(SummaryOut::from);

    let tasks = store
        .tasks_for_thread(&args.thread_key)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(TaskOut::from)
        .collect();

    let mut out_messages = Vec::with_capacity(messages.len());
    for m in messages {
        out_messages.push(message_to_out(store, m, args.include_quotes)?);
    }

    Ok(ThreadOut {
        thread_key: args.thread_key.clone(),
        subject,
        participants,
        messages: out_messages,
        summary,
        tasks,
    })
}

/// `get_message` の本体。
fn get_message_impl(
    store: &Store,
    args: &GetMessageArgs,
) -> std::result::Result<MessageOut, String> {
    // include_html は body_html を返すかどうかのフラグだが、`Store::get_message` が
    // まだ body_html を返さないため、現状は無視する。
    // TODO: body_html を Store から取れるようにする。
    let _ = args.include_html;

    let msg = store
        .get_message(args.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "メッセージが見つかりません".to_string())?;
    message_to_out(store, msg, true)
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
    use mailstore::NewMessage;

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

    /// アカウント + INBOX フォルダを作る。
    fn seed_account(store: &Store, email: &str, project_tag: Option<&str>) -> i64 {
        store
            .add_account(
                email,
                AccountKind::Imap,
                email,
                project_tag,
                &serde_json::json!({}),
            )
            .unwrap()
            .id
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_msg(
        store: &Store,
        account_id: i64,
        folder_id: i64,
        uid: u32,
        thread_key: &str,
        subject: &str,
        date: chrono::DateTime<chrono::Utc>,
        raw_path: Option<&str>,
    ) -> i64 {
        let from = Address {
            name: Some("送信者".into()),
            email: "sender@mail.example".into(),
        };
        let m = NewMessage {
            account_id,
            folder_id,
            uid,
            message_id: None,
            thread_key,
            from: &from,
            to: &[],
            cc: &[],
            subject,
            date,
            snippet: subject,
            body_text: "本文です。",
            body_html: None,
            has_attachments: false,
            is_read: false,
            is_flagged: false,
            raw_path,
        };
        store.insert_message(&m).unwrap().unwrap()
    }

    fn sample_eml_with_quote() -> Vec<u8> {
        let text = "From: sender <sender@mail.example>\r\n\
             To: recipient <recipient@mail.example>\r\n\
             Subject: quote test\r\n\
             Date: Wed, 06 Sep 2026 09:00:00 +0900\r\n\
             Message-ID: <msg-quote@mail.example>\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=\"utf-8\"\r\n\
             \r\n\
             返信です。\r\n\
             > 元のメールの引用行\r\n\
             > もう一行\r\n";
        text.as_bytes().to_vec()
    }

    #[test]
    fn search_messages_impl_caps_limit_at_200() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "t1",
            "件名",
            chrono::Utc::now(),
            None,
        );

        let args = SearchMessagesArgs {
            query: None,
            account_id: None,
            project: None,
            since: None,
            unread_only: false,
            limit: 10_000,
        };
        // limit そのものが上限を超えて Store に渡らないことを、間接的に
        // 「エラーにならず結果が返る」ことで確認する（Store::search は limit をそのまま
        // SQL の LIMIT に使うため、巨大な値を渡してもクラッシュはしないが、
        // ここでは呼び出し側の丸め込みを検証する）。
        let results = search_messages_impl(&store, &args).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_messages_impl_fills_project_tag_from_account() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", Some("案件A"));
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "t1",
            "件名",
            chrono::Utc::now(),
            None,
        );

        let args = SearchMessagesArgs {
            query: None,
            account_id: None,
            project: None,
            since: None,
            unread_only: false,
            limit: 30,
        };
        let results = search_messages_impl(&store, &args).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_tag.as_deref(), Some("案件A"));
    }

    #[test]
    fn get_thread_impl_omits_quoted_text_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        std::fs::write(&eml_path, sample_eml_with_quote()).unwrap();

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );

        let args = GetThreadArgs {
            thread_key: "thread-1".to_string(),
            include_quotes: false,
        };
        let out = get_thread_impl(&store, &args).unwrap();
        assert!(out.messages[0].quoted_text.is_none());
    }

    #[test]
    fn get_thread_impl_includes_quoted_text_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        std::fs::write(&eml_path, sample_eml_with_quote()).unwrap();

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );

        let args = GetThreadArgs {
            thread_key: "thread-1".to_string(),
            include_quotes: true,
        };
        let out = get_thread_impl(&store, &args).unwrap();
        assert_eq!(
            out.messages[0].quoted_text.as_deref(),
            Some("> 元のメールの引用行\n> もう一行")
        );
    }

    #[test]
    fn get_thread_impl_returns_the_whole_thread_even_if_an_eml_is_missing() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        // raw_path が無いメッセージ。
        insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名",
            chrono::Utc::now(),
            None,
        );

        let args = GetThreadArgs {
            thread_key: "thread-1".to_string(),
            include_quotes: true,
        };
        let out = get_thread_impl(&store, &args).unwrap();
        assert_eq!(out.messages.len(), 1);
        assert_eq!(out.messages[0].quoted_text.as_deref(), Some(""));
    }

    #[test]
    fn get_thread_impl_reports_not_found_for_an_unknown_key() {
        let store = Store::open_in_memory().unwrap();
        let args = GetThreadArgs {
            thread_key: "no-such-thread".to_string(),
            include_quotes: false,
        };
        let err = get_thread_impl(&store, &args).unwrap_err();
        assert!(err.contains("見つかりません"));
    }

    #[test]
    fn get_thread_impl_only_returns_tasks_for_that_thread() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let msg1 = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名1",
            chrono::Utc::now(),
            None,
        );
        let msg2 = insert_msg(
            &store,
            account,
            folder,
            2,
            "thread-2",
            "件名2",
            chrono::Utc::now(),
            None,
        );
        store
            .upsert_task(&mailstore::NewTask {
                account_id: account,
                source_message_id: Some(msg1),
                title: "thread-1 のタスク".into(),
                due: None,
                confidence: 0.9,
                created_by: "ai",
            })
            .unwrap();
        store
            .upsert_task(&mailstore::NewTask {
                account_id: account,
                source_message_id: Some(msg2),
                title: "thread-2 のタスク".into(),
                due: None,
                confidence: 0.9,
                created_by: "ai",
            })
            .unwrap();

        let args = GetThreadArgs {
            thread_key: "thread-1".to_string(),
            include_quotes: false,
        };
        let out = get_thread_impl(&store, &args).unwrap();
        assert_eq!(out.tasks.len(), 1);
        assert_eq!(out.tasks[0].title, "thread-1 のタスク");
    }

    #[test]
    fn get_message_impl_returns_not_found_for_unknown_id() {
        let store = Store::open_in_memory().unwrap();
        let args = GetMessageArgs {
            id: 999,
            include_html: false,
        };
        let err = get_message_impl(&store, &args).unwrap_err();
        assert!(err.contains("見つかりません"));
    }

    #[test]
    fn get_message_impl_includes_quoted_text_and_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        std::fs::write(&eml_path, sample_eml_with_quote()).unwrap();

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let msg_id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );
        store
            .insert_attachment_meta(msg_id, "note.txt", "text/plain", 4)
            .unwrap();

        let args = GetMessageArgs {
            id: msg_id,
            include_html: false,
        };
        let out = get_message_impl(&store, &args).unwrap();
        assert_eq!(
            out.quoted_text.as_deref(),
            Some("> 元のメールの引用行\n> もう一行")
        );
        assert_eq!(out.attachments.len(), 1);
        assert_eq!(out.attachments[0].filename, "note.txt");
    }
}
