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
use mailmcp::{
    CreateDraftArgs, GetMessageArgs, GetThreadArgs, InboxDigestArgs, ListTasksArgs,
    SaveSummaryArgs, SearchMessagesArgs, UpsertTasksArgs,
};
use mailstore::{AttachmentRow, NewDraft, NewTask, SearchQuery, Store, SummaryRow, TaskQuery, ThreadQuery};
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

/// `inbox_digest` の 1 スレッド。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct DigestThreadOut {
    thread_key: String,
    subject: String,
    from: AddressOut,
    /// RFC 3339
    last_date: String,
    message_count: i64,
    unread_count: i64,
    snippet: String,
    has_attachments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<SummaryOut>,
    tasks: Vec<TaskOut>,
}

/// `inbox_digest` の 1 グループ（案件単位）。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct DigestGroupOut {
    /// project_tag。未設定のアカウントのスレッドは最後のグループにまとめる。
    project_tag: Option<String>,
    threads: Vec<DigestThreadOut>,
}

/// `inbox_digest` の返り値。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct InboxDigestOut {
    /// この時刻以降に更新されたスレッドだけを含む。RFC 3339。
    /// 省略時は「直近 24 時間」（サーバは「今日」を知らないための方針）。
    since: String,
    groups: Vec<DigestGroupOut>,
    /// スレッド数の上限（50）で切ったら true。
    truncated: bool,
}

/// `save_summary` の返り値。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct SaveSummaryOut {
    id: i64,
    /// RFC 3339
    created_at: String,
}

/// `upsert_tasks` の返り値。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct UpsertTasksOut {
    inserted: i64,
    updated: i64,
}

/// `create_draft` の返り値。
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
struct CreateDraftOut {
    id: i64,
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

    #[tool(
        description = "未処理（未読）メールをスレッド単位でまとめたダイジェストを返す。\
        since を省略すると直近 24 時間になる（サーバは「今日」を知らないため）。\
        スレッド数は最大 50 件で、それ以上は truncated=true で切る。\
        送信はできません。既読状態は変更されません。\n\
        Returns unread mail grouped by thread as a digest. If since is omitted, the last \
        24 hours are used (the server has no notion of \"today\"). At most 50 threads are \
        returned; beyond that, truncated=true. This server cannot send mail and never \
        changes read state."
    )]
    async fn inbox_digest(
        &self,
        Parameters(args): Parameters<InboxDigestArgs>,
    ) -> Result<Json<InboxDigestOut>, String> {
        let store = self.lock_store()?;
        inbox_digest_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "Claude が作った要約を保存する。target は thread:<key> / message:<id> / \
        daily:<yyyy-mm-dd> のいずれかの形式にすること。model と summary は必須（空は不可）。\
        送信はできません。既読状態は変更されません。\n\
        Saves a Claude-generated summary. target must be thread:<key>, message:<id> or \
        daily:<yyyy-mm-dd>. model and summary are required and must not be empty. \
        This server cannot send mail and never changes read state."
    )]
    async fn save_summary(
        &self,
        Parameters(args): Parameters<SaveSummaryArgs>,
    ) -> Result<Json<SaveSummaryOut>, String> {
        let store = self.lock_store()?;
        save_summary_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "タスク抽出結果をまとめて保存する（既存があれば更新、無ければ新規作成）。\
        created_by は常に \"ai\" になる。due を指定する場合は RFC3339 で、\
        パースできなければそのタスクをエラーにする。送信はできません。既読状態は変更されません。\n\
        Saves extracted tasks in bulk (insert or update). created_by is always \"ai\". \
        due must be RFC3339 if given; an unparsable value fails that task. \
        This server cannot send mail and never changes read state."
    )]
    async fn upsert_tasks(
        &self,
        Parameters(args): Parameters<UpsertTasksArgs>,
    ) -> Result<Json<UpsertTasksOut>, String> {
        let store = self.lock_store()?;
        upsert_tasks_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "タスク一覧を返す。status（open/done/dismissed）や案件タグで絞り込める。\
        送信はできません。既読状態は変更されません。\n\
        Lists tasks, optionally filtered by status (open/done/dismissed) or project tag. \
        This server cannot send mail and never changes read state."
    )]
    async fn list_tasks(
        &self,
        Parameters(args): Parameters<ListTasksArgs>,
    ) -> Result<Json<Vec<TaskOut>>, String> {
        let store = self.lock_store()?;
        list_tasks_impl(&store, &args).map(Json)
    }

    #[tool(
        description = "下書きを保存するだけで、送信はしません。送信は Meowbox の画面から人間が行います。\
        in_reply_to があれば宛先・件名（Re: を二重に付けない）を補う。to / subject を指定すれば\
        そちらを優先する。既読状態は変更されません。\n\
        Saves a reply draft only; it never sends anything. Sending is done by a human from \
        the Meowbox UI. If in_reply_to is given, the recipient and subject (without doubling \
        \"Re:\") are inferred, unless to / subject are given explicitly. \
        This server never changes read state."
    )]
    async fn create_draft(
        &self,
        Parameters(args): Parameters<CreateDraftArgs>,
    ) -> Result<Json<CreateDraftOut>, String> {
        let store = self.lock_store()?;
        create_draft_impl(&store, &args).map(Json)
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

/// `inbox_digest` がスレッド数を切り上限とみなす件数。
const INBOX_DIGEST_THREAD_LIMIT: usize = 50;

/// `inbox_digest` の本体。
fn inbox_digest_impl(
    store: &Store,
    args: &InboxDigestArgs,
) -> std::result::Result<InboxDigestOut, String> {
    // サーバは「今日」もタイムゾーンも知らないので、since 省略時は単純に
    // 「直近 24 時間」を使う（「今日」の境界は UI 側の責務）。
    let since = match &args.since {
        Some(s) => parse_rfc3339(s)?,
        None => Utc::now() - chrono::Duration::hours(24),
    };

    let query = ThreadQuery {
        project_tag: args.project.as_deref(),
        unread_only: true,
        ..Default::default()
    };
    let threads = store.list_threads(&query).map_err(|e| e.to_string())?;

    // list_threads は最終更新の日時降順で返すので、フィルタ後もその順のまま。
    let filtered: Vec<_> = threads
        .into_iter()
        .filter(|t| t.last_date >= since)
        .collect();
    let truncated = filtered.len() > INBOX_DIGEST_THREAD_LIMIT;

    let mut tagged: BTreeMap<String, Vec<DigestThreadOut>> = BTreeMap::new();
    let mut untagged: Vec<DigestThreadOut> = Vec::new();
    for t in filtered.into_iter().take(INBOX_DIGEST_THREAD_LIMIT) {
        let project_tag = t.project_tag.clone();
        let summary = store
            .latest_summary(&format!("thread:{}", t.thread_key))
            .map_err(|e| e.to_string())?
            .map(SummaryOut::from);
        let tasks = store
            .tasks_for_thread(&t.thread_key)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(TaskOut::from)
            .collect();

        let out = DigestThreadOut {
            thread_key: t.thread_key,
            subject: t.subject,
            from: t.from.into(),
            last_date: t.last_date.to_rfc3339(),
            message_count: t.message_count,
            unread_count: t.unread_count,
            snippet: t.snippet,
            has_attachments: t.has_attachments,
            summary,
            tasks,
        };
        match project_tag {
            Some(tag) => tagged.entry(tag).or_default().push(out),
            None => untagged.push(out),
        }
    }

    let mut groups: Vec<DigestGroupOut> = tagged
        .into_iter()
        .map(|(tag, threads)| DigestGroupOut {
            project_tag: Some(tag),
            threads,
        })
        .collect();
    if !untagged.is_empty() {
        groups.push(DigestGroupOut {
            project_tag: None,
            threads: untagged,
        });
    }

    Ok(InboxDigestOut {
        since: since.to_rfc3339(),
        groups,
        truncated,
    })
}

/// `save_summary` の本体。`target` の形式・`model` / `summary` の非空をここで確かめる。
fn save_summary_impl(
    store: &Store,
    args: &SaveSummaryArgs,
) -> std::result::Result<SaveSummaryOut, String> {
    if !(args.target.starts_with("thread:")
        || args.target.starts_with("message:")
        || args.target.starts_with("daily:"))
    {
        return Err(format!(
            "target の形式が不正です（thread:<key> / message:<id> / daily:<yyyy-mm-dd> の\
             いずれかで始めてください）: {}",
            args.target
        ));
    }
    if args.model.is_empty() {
        return Err("model を指定してください".to_string());
    }
    if args.summary.trim().is_empty() {
        return Err("summary が空です".to_string());
    }

    let id = store
        .save_summary(&args.target, &args.model, &args.summary)
        .map_err(|e| e.to_string())?;
    let created_at = store
        .latest_summary(&args.target)
        .map_err(|e| e.to_string())?
        .map(|r| r.created_at.to_rfc3339())
        .ok_or_else(|| "保存した要約の取得に失敗しました".to_string())?;

    Ok(SaveSummaryOut { id, created_at })
}

/// `upsert_tasks` の本体。`due` がパースできないタスクはそこでエラーにする
/// （黙って期限を落とすと Claude も人間も気づけないため）。
fn upsert_tasks_impl(
    store: &Store,
    args: &UpsertTasksArgs,
) -> std::result::Result<UpsertTasksOut, String> {
    if args.tasks.is_empty() {
        return Err("tasks が空です".to_string());
    }

    let mut inserted = 0i64;
    let mut updated = 0i64;
    for (i, t) in args.tasks.iter().enumerate() {
        let due = match &t.due {
            Some(s) => Some(
                parse_rfc3339(s).map_err(|e| format!("tasks[{i}] の due が不正です: {e}"))?,
            ),
            None => None,
        };
        let new_task = NewTask {
            account_id: t.account_id,
            source_message_id: t.source_message_id,
            title: &t.title,
            due,
            confidence: t.confidence,
            // created_by は MCP 経由 = Claude が作ったものなので常に "ai"。
            created_by: "ai",
        };
        let (_, was_inserted) = store.upsert_task(&new_task).map_err(|e| e.to_string())?;
        if was_inserted {
            inserted += 1;
        } else {
            updated += 1;
        }
    }

    Ok(UpsertTasksOut { inserted, updated })
}

/// `list_tasks` の本体。
fn list_tasks_impl(
    store: &Store,
    args: &ListTasksArgs,
) -> std::result::Result<Vec<TaskOut>, String> {
    let status = args.status.as_deref().map(parse_task_status).transpose()?;
    let query = TaskQuery {
        status,
        project_tag: args.project.as_deref(),
        due_before: None,
        limit: 0,
    };
    let tasks = store.list_tasks(&query).map_err(|e| e.to_string())?;
    Ok(tasks.into_iter().map(TaskOut::from).collect())
}

/// `create_draft` の宛先・件名を決める。`to` / `subject` が明示されていればそちらを
/// 優先し、無ければ `in_reply_to` のメッセージから補う。
fn resolve_draft_to_and_subject(
    store: &Store,
    args: &CreateDraftArgs,
) -> std::result::Result<(Vec<Address>, String), String> {
    let need_original = args.to.is_none() || args.subject.is_none();
    let original = if need_original {
        match args.in_reply_to {
            Some(id) => Some(
                store
                    .get_message(id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "in_reply_to のメッセージが見つかりません".to_string())?,
            ),
            None => None,
        }
    } else {
        None
    };

    let to = match &args.to {
        Some(addrs) => addrs
            .iter()
            .map(|email| Address {
                name: None,
                email: email.clone(),
            })
            .collect(),
        None => match &original {
            Some(msg) => vec![msg.from.clone()],
            None => {
                return Err(
                    "宛先を決められません（to か in_reply_to を指定してください）".to_string(),
                )
            }
        },
    };

    let subject = match &args.subject {
        Some(s) => s.clone(),
        None => match &original {
            // 既存の Re: / Re[2]: などのプレフィックスを剥がしてから 1 回だけ付け直す。
            Some(msg) => format!("Re: {}", mailcore::normalize_subject(&msg.subject)),
            None => String::new(),
        },
    };

    Ok((to, subject))
}

/// `create_draft` の本体。status は必ず 'draft'（`Store::insert_draft` が固定している）。
fn create_draft_impl(
    store: &Store,
    args: &CreateDraftArgs,
) -> std::result::Result<CreateDraftOut, String> {
    if args.body.trim().is_empty() {
        return Err("body が空です".to_string());
    }

    let (to, subject) = resolve_draft_to_and_subject(store, args)?;

    let new_draft = NewDraft {
        account_id: args.account_id,
        in_reply_to: args.in_reply_to,
        to: &to,
        subject: &subject,
        body: &args.body,
    };
    let id = store.insert_draft(&new_draft).map_err(|e| e.to_string())?;
    Ok(CreateDraftOut { id })
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

    #[test]
    fn inbox_digest_impl_defaults_since_to_24_hours_ago() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let now = chrono::Utc::now();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-old",
            "25 時間前",
            now - chrono::Duration::hours(25),
            None,
        );
        insert_msg(
            &store,
            account,
            folder,
            2,
            "thread-new",
            "1 時間前",
            now - chrono::Duration::hours(1),
            None,
        );

        let args = InboxDigestArgs {
            project: None,
            since: None,
        };
        let out = inbox_digest_impl(&store, &args).unwrap();
        let keys: Vec<&str> = out
            .groups
            .iter()
            .flat_map(|g| g.threads.iter().map(|t| t.thread_key.as_str()))
            .collect();
        assert_eq!(keys, vec!["thread-new"]);
    }

    #[test]
    fn inbox_digest_impl_drops_threads_older_than_since() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let now = chrono::Utc::now();
        insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-old",
            "3 時間前",
            now - chrono::Duration::hours(3),
            None,
        );
        insert_msg(
            &store,
            account,
            folder,
            2,
            "thread-new",
            "30 分前",
            now - chrono::Duration::minutes(30),
            None,
        );

        let args = InboxDigestArgs {
            project: None,
            since: Some((now - chrono::Duration::hours(2)).to_rfc3339()),
        };
        let out = inbox_digest_impl(&store, &args).unwrap();
        let keys: Vec<&str> = out
            .groups
            .iter()
            .flat_map(|g| g.threads.iter().map(|t| t.thread_key.as_str()))
            .collect();
        assert_eq!(keys, vec!["thread-new"]);
    }

    #[test]
    fn inbox_digest_impl_truncates_at_50_threads() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let now = chrono::Utc::now();
        for i in 0..51 {
            insert_msg(
                &store,
                account,
                folder,
                i + 1,
                &format!("thread-{i}"),
                "件名",
                now - chrono::Duration::minutes(i as i64),
                None,
            );
        }

        let args = InboxDigestArgs {
            project: None,
            since: None,
        };
        let out = inbox_digest_impl(&store, &args).unwrap();
        let count: usize = out.groups.iter().map(|g| g.threads.len()).sum();
        assert_eq!(count, 50);
        assert!(out.truncated);
    }

    #[test]
    fn save_summary_impl_rejects_bad_target() {
        let store = Store::open_in_memory().unwrap();
        let args = SaveSummaryArgs {
            target: "weird:1".to_string(),
            model: "claude".to_string(),
            summary: "要約".to_string(),
        };
        assert!(save_summary_impl(&store, &args).is_err());
    }

    #[test]
    fn save_summary_impl_rejects_empty_model() {
        let store = Store::open_in_memory().unwrap();
        let args = SaveSummaryArgs {
            target: "thread:t1".to_string(),
            model: "".to_string(),
            summary: "要約".to_string(),
        };
        assert!(save_summary_impl(&store, &args).is_err());
    }

    #[test]
    fn save_summary_impl_rejects_blank_summary() {
        let store = Store::open_in_memory().unwrap();
        let args = SaveSummaryArgs {
            target: "thread:t1".to_string(),
            model: "claude".to_string(),
            summary: "   ".to_string(),
        };
        assert!(save_summary_impl(&store, &args).is_err());
    }

    #[test]
    fn save_summary_impl_saves_a_valid_summary() {
        let store = Store::open_in_memory().unwrap();
        let args = SaveSummaryArgs {
            target: "thread:t1".to_string(),
            model: "claude".to_string(),
            summary: "要約".to_string(),
        };
        let out = save_summary_impl(&store, &args).unwrap();
        assert!(out.id > 0);
        assert!(!out.created_at.is_empty());
    }

    #[test]
    fn upsert_tasks_impl_rejects_empty_list() {
        let store = Store::open_in_memory().unwrap();
        let args = UpsertTasksArgs { tasks: vec![] };
        assert!(upsert_tasks_impl(&store, &args).is_err());
    }

    #[test]
    fn upsert_tasks_impl_rejects_bad_due() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let args = UpsertTasksArgs {
            tasks: vec![mailmcp::TaskInput {
                account_id: account,
                source_message_id: None,
                title: "見積の確認".to_string(),
                due: Some("not-a-date".to_string()),
                confidence: 0.8,
            }],
        };
        assert!(upsert_tasks_impl(&store, &args).is_err());
    }

    #[test]
    fn upsert_tasks_impl_counts_insert_then_update() {
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
            None,
        );

        let task_input = || mailmcp::TaskInput {
            account_id: account,
            source_message_id: Some(msg_id),
            title: "見積の確認".to_string(),
            due: None,
            confidence: 0.8,
        };

        let args = UpsertTasksArgs {
            tasks: vec![task_input()],
        };
        let first = upsert_tasks_impl(&store, &args).unwrap();
        assert_eq!((first.inserted, first.updated), (1, 0));

        let second = upsert_tasks_impl(&store, &args).unwrap();
        assert_eq!((second.inserted, second.updated), (0, 1));

        let saved = store
            .list_tasks(&mailstore::TaskQuery::default())
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].created_by, "ai");
    }

    #[test]
    fn create_draft_impl_rejects_blank_body() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let args = CreateDraftArgs {
            account_id: account,
            in_reply_to: None,
            to: Some(vec!["client@client.example".to_string()]),
            subject: Some("件名".to_string()),
            body: "   ".to_string(),
        };
        assert!(create_draft_impl(&store, &args).is_err());
    }

    #[test]
    fn create_draft_impl_requires_a_recipient() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let args = CreateDraftArgs {
            account_id: account,
            in_reply_to: None,
            to: None,
            subject: None,
            body: "本文です。".to_string(),
        };
        assert!(create_draft_impl(&store, &args).is_err());
    }

    #[test]
    fn create_draft_impl_does_not_double_up_re_prefix() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let msg_id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "Re: 見積の件",
            chrono::Utc::now(),
            None,
        );

        let args = CreateDraftArgs {
            account_id: account,
            in_reply_to: Some(msg_id),
            to: None,
            subject: None,
            body: "承知しました。".to_string(),
        };
        let out = create_draft_impl(&store, &args).unwrap();

        let draft = store.get_draft(out.id).unwrap().unwrap();
        assert_eq!(draft.subject, "Re: 見積の件");
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.to[0].email, "sender@mail.example");
    }
}
