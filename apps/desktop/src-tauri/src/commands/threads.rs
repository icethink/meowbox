//! スレッド・メッセージ・添付・ダイジェストのコマンド。ロジックは `&Store`
//! （と必要なら添付ディレクトリのパス）を取る素の関数に書き、`#[tauri::command]`
//! はそれを呼ぶ薄いラッパにする。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use mailcore::{Task, TaskStatus};
use mailstore::{Store, SummaryRow, TaskQuery, ThreadQuery};

use crate::error::AppError;
use crate::state::AppState;

/// スレッド一覧の絞り込み。ビュー（すべて/未読/フラグ）の解釈は UI 側で
/// これらのフラグに落としてから渡す。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ThreadFilter {
    pub project_tag: Option<String>,
    pub account_id: Option<i64>,
    pub unread_only: bool,
    pub flagged_only: bool,
    pub include_archived: bool,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// スレッド 1 本ぶん。本文と引用を含む。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThreadDetailDto {
    pub thread_key: String,
    /// 最新メッセージの件名。
    pub subject: String,
    pub project_tag: Option<String>,
    pub messages: Vec<MessageDto>,
    /// Claude が保存した要約（`thread:<key>`）。まだ無ければ None。
    pub summary: Option<SummaryDto>,
    /// このスレッドのメッセージから抽出されたタスク。まだ無ければ空。
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MessageDto {
    pub id: i64,
    pub account_id: i64,
    pub thread_key: String,
    pub from: mailcore::Address,
    pub to: Vec<mailcore::Address>,
    pub cc: Vec<mailcore::Address>,
    pub subject: String,
    /// RFC 3339。表示用のラベルは UI が作る。
    pub date: String,
    /// 引用・署名を落とした本文（DB の body_text）。
    pub body_text: String,
    /// raw .eml から読み直した引用・署名部分。読めなければ空文字列。
    pub quoted_text: String,
    pub has_attachments: bool,
    pub is_read: bool,
    pub is_flagged: bool,
    pub attachments: Vec<AttachmentDto>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentDto {
    pub id: i64,
    pub filename: String,
    pub mime: String,
    pub size: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryDto {
    pub target: String,
    pub model: String,
    pub summary: String,
    pub created_at: String,
}

/// サイドバーの「ダイジェスト」に出す 1 案件ぶんのグループ。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DigestGroupDto {
    pub project_tag: Option<String>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DigestDto {
    /// Claude が保存した日次要約（`daily:<date_key>`）。まだ無ければ None。
    pub summary: Option<SummaryDto>,
    pub groups: Vec<DigestGroupDto>,
}

/// サイドバーの「ビュー」に出す件数。mailstore の ViewCounts は Serialize を
/// 持たないのでここで詰め替える。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ViewCountsDto {
    pub all: i64,
    pub unread: i64,
    pub flagged: i64,
    pub tasks: i64,
    pub drafts: i64,
}

fn summary_to_dto(row: SummaryRow) -> SummaryDto {
    SummaryDto {
        target: row.target,
        model: row.model,
        summary: row.summary,
        created_at: row.created_at.to_rfc3339(),
    }
}

/// raw .eml を読み直して引用・署名部分を取り出す。読み込み・パースに失敗しても
/// 空文字列を返すだけにする（1 通の壊れたメールで画面全体を落とさないため）。
/// 失敗の詳細は `tracing::debug!` に留め、パスや本文はログに出さない。
fn quoted_text_for_message(store: &Store, message_id: i64) -> String {
    let raw_path = match store.message_raw_path(message_id) {
        Ok(Some(path)) => path,
        Ok(None) => return String::new(),
        Err(_) => {
            tracing::debug!(message_id, "raw_path の取得に失敗しました");
            return String::new();
        }
    };

    let raw = match fs::read(&raw_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            tracing::debug!(message_id, "raw .eml の読み込みに失敗しました");
            return String::new();
        }
    };

    match mailsync::parse::parse(&raw) {
        Ok(parsed) => parsed.quoted_text,
        Err(_) => {
            tracing::debug!(message_id, "raw .eml のパースに失敗しました");
            String::new()
        }
    }
}

/// `mailcore::Message` + 添付一覧 + 引用部分を `MessageDto` に詰め替える。
/// `get_thread` / `get_message` の両方から呼ぶ共通処理。
fn message_to_dto(store: &Store, msg: mailcore::Message) -> Result<MessageDto, AppError> {
    let quoted_text = quoted_text_for_message(store, msg.id);
    let attachments = store
        .list_attachments(msg.id)?
        .into_iter()
        .map(|a| AttachmentDto {
            id: a.id,
            filename: a.filename,
            mime: a.mime,
            size: a.size,
        })
        .collect();

    Ok(MessageDto {
        id: msg.id,
        account_id: msg.account_id,
        thread_key: msg.thread_key,
        from: msg.from,
        to: msg.to,
        cc: msg.cc,
        subject: msg.subject,
        date: msg.date.to_rfc3339(),
        body_text: msg.body_text,
        quoted_text,
        has_attachments: msg.has_attachments,
        is_read: msg.is_read,
        is_flagged: msg.is_flagged,
        attachments,
    })
}

/// `list_threads` の絞り込み本体。
fn list_threads_impl(
    store: &Store,
    filter: ThreadFilter,
) -> Result<Vec<mailcore::ThreadSummary>, AppError> {
    let query = ThreadQuery {
        account_id: filter.account_id,
        project_tag: filter.project_tag.as_deref(),
        unread_only: filter.unread_only,
        flagged_only: filter.flagged_only,
        include_archived: filter.include_archived,
        limit: filter.limit.unwrap_or(200),
        offset: filter.offset.unwrap_or(0),
    };
    store.list_threads(&query).map_err(AppError::from)
}

/// `get_thread` の組み立て本体。
fn get_thread_impl(store: &Store, thread_key: &str) -> Result<ThreadDetailDto, AppError> {
    let messages = store.thread_messages(thread_key)?;
    let latest = messages
        .last()
        .ok_or_else(|| AppError::not_found("スレッドが見つかりません"))?;

    let subject = latest.subject.clone();
    let project_tag = store
        .get_account(latest.account_id)?
        .and_then(|a| a.project_tag);

    let message_ids: HashSet<i64> = messages.iter().map(|m| m.id).collect();

    let summary = store
        .latest_summary(&format!("thread:{thread_key}"))?
        .map(summary_to_dto);

    let tasks = store
        .list_tasks(&TaskQuery {
            status: Some(TaskStatus::Open),
            limit: 200,
            ..Default::default()
        })?
        .into_iter()
        .filter(|t| {
            t.source_message_id
                .is_some_and(|id| message_ids.contains(&id))
        })
        .collect();

    let mut dtos = Vec::with_capacity(messages.len());
    for msg in messages {
        dtos.push(message_to_dto(store, msg)?);
    }

    Ok(ThreadDetailDto {
        thread_key: thread_key.to_string(),
        subject,
        project_tag,
        messages: dtos,
        summary,
        tasks,
    })
}

/// `get_message` の組み立て本体。
fn get_message_impl(store: &Store, id: i64) -> Result<MessageDto, AppError> {
    let msg = store
        .get_message(id)?
        .ok_or_else(|| AppError::not_found("メッセージが見つかりません"))?;
    message_to_dto(store, msg)
}

/// 無害化した添付ファイル名が空・`_`・`__` に潰れた場合の代わりの名前。
fn fallback_attachment_name(id: i64) -> String {
    format!("attachment-{id}")
}

/// `extract_attachment` の本体。添付を .eml から取り出してファイルに書き、
/// そのパスを返す。2 回目以降は書き出し済みのファイルを再利用する。
fn extract_attachment_impl(
    store: &Store,
    attachments_dir: &Path,
    id: i64,
) -> Result<String, AppError> {
    let attachment = store
        .get_attachment(id)?
        .ok_or_else(|| AppError::not_found("添付が見つかりません"))?;

    if let Some(existing) = &attachment.path {
        if Path::new(existing).exists() {
            return Ok(existing.clone());
        }
    }

    let raw_path = store
        .message_raw_path(attachment.message_id)?
        .ok_or_else(|| AppError::not_found("元のメールのファイルが見つかりません"))?;

    let raw = fs::read(&raw_path)
        .map_err(|_| AppError::not_found("元のメールのファイルが見つかりません"))?;

    let index = store
        .attachment_index(id)?
        .ok_or_else(|| AppError::not_found("添付が見つかりません"))?;

    let bytes = mailsync::parse::attachment_bytes(&raw, index)
        .map_err(|_| AppError::internal("添付の取り出しに失敗しました"))?;

    let safe_name = mailsync::fsname::sanitize_path_segment(&attachment.filename);
    let safe_name = if matches!(safe_name.as_str(), "" | "_" | "__") {
        fallback_attachment_name(id)
    } else {
        safe_name
    };

    let dir = attachments_dir.join(attachment.message_id.to_string());
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::internal(format!("添付の保存先を作成できませんでした: {e}")))?;

    let path = dir.join(safe_name);
    fs::write(&path, &bytes)
        .map_err(|e| AppError::internal(format!("添付の保存に失敗しました: {e}")))?;

    let path_str = path.to_string_lossy().to_string();
    store.set_attachment_path(id, &path_str)?;

    Ok(path_str)
}

/// `get_digest` の本体。
///
/// タスクも要約もまだ実データが無いので、いまは空で返るのが正しい。
/// `start_of_day` は現状 `list_tasks` の絞りには使っていない（期限なしタスクも
/// 出したいため）、形式チェックだけに使う。
/// TODO(P1): その日に届いたスレッドの未処理分もここに混ぜる
fn get_digest_impl(
    store: &Store,
    start_of_day: &str,
    date_key: &str,
) -> Result<DigestDto, AppError> {
    chrono::DateTime::parse_from_rfc3339(start_of_day)
        .map_err(|_| AppError::invalid_input("start_of_day の形式が不正です"))?;

    let summary = store
        .latest_summary(&format!("daily:{date_key}"))?
        .map(summary_to_dto);

    let tasks = store.list_tasks(&TaskQuery {
        status: Some(TaskStatus::Open),
        limit: 200,
        ..Default::default()
    })?;

    let mut tagged: BTreeMap<String, Vec<Task>> = BTreeMap::new();
    let mut untagged: Vec<Task> = Vec::new();
    for task in tasks {
        let project_tag = store
            .get_account(task.account_id)?
            .and_then(|a| a.project_tag);
        match project_tag {
            Some(tag) => tagged.entry(tag).or_default().push(task),
            None => untagged.push(task),
        }
    }

    let mut groups: Vec<DigestGroupDto> = tagged
        .into_iter()
        .map(|(tag, tasks)| DigestGroupDto {
            project_tag: Some(tag),
            tasks,
        })
        .collect();
    if !untagged.is_empty() {
        groups.push(DigestGroupDto {
            project_tag: None,
            tasks: untagged,
        });
    }

    Ok(DigestDto { summary, groups })
}

/// `mark` の本体。`action` が未知なら `invalid_input`。
fn mark_impl(store: &Store, ids: &[i64], action: &str) -> Result<usize, AppError> {
    match action {
        "read" => store.set_read(ids, true).map_err(AppError::from),
        "unread" => store.set_read(ids, false).map_err(AppError::from),
        "flag" => store.set_flagged(ids, true).map_err(AppError::from),
        "unflag" => store.set_flagged(ids, false).map_err(AppError::from),
        "archive" => store.set_archived(ids, true).map_err(AppError::from),
        "unarchive" => store.set_archived(ids, false).map_err(AppError::from),
        other => Err(AppError::invalid_input(format!("不明な操作です: {other}"))),
    }
}

fn view_counts_impl(store: &Store) -> Result<ViewCountsDto, AppError> {
    let counts = store.view_counts()?;
    Ok(ViewCountsDto {
        all: counts.all,
        unread: counts.unread,
        flagged: counts.flagged,
        tasks: counts.tasks,
        drafts: counts.drafts,
    })
}

#[tauri::command]
pub fn list_threads(
    state: tauri::State<'_, AppState>,
    filter: ThreadFilter,
) -> Result<Vec<mailcore::ThreadSummary>, AppError> {
    let store = state.store()?;
    list_threads_impl(&store, filter)
}

#[tauri::command]
pub fn get_thread(
    state: tauri::State<'_, AppState>,
    thread_key: String,
) -> Result<ThreadDetailDto, AppError> {
    let store = state.store()?;
    get_thread_impl(&store, &thread_key)
}

#[tauri::command]
pub fn get_message(state: tauri::State<'_, AppState>, id: i64) -> Result<MessageDto, AppError> {
    let store = state.store()?;
    get_message_impl(&store, id)
}

/// 添付を .eml から取り出してファイルに書き、そのパスを返す。
/// 2 回目以降は書き出し済みのファイルを再利用する。
#[tauri::command]
pub fn extract_attachment(state: tauri::State<'_, AppState>, id: i64) -> Result<String, AppError> {
    let store = state.store()?;
    extract_attachment_impl(&store, &state.attachments_dir(), id)
}

#[tauri::command]
pub fn get_digest(
    state: tauri::State<'_, AppState>,
    start_of_day: String,
    date_key: String,
) -> Result<DigestDto, AppError> {
    let store = state.store()?;
    get_digest_impl(&store, &start_of_day, &date_key)
}

/// 既読・フラグ・アーカイブを変える。**IMAP には書き戻さない**（DB のみ）。
/// TODO(P4): IMAP の FLAGS に反映する。
#[tauri::command]
pub fn mark(
    state: tauri::State<'_, AppState>,
    ids: Vec<i64>,
    action: String,
) -> Result<usize, AppError> {
    let store = state.store()?;
    mark_impl(&store, &ids, &action)
}

#[tauri::command]
pub fn view_counts(state: tauri::State<'_, AppState>) -> Result<ViewCountsDto, AppError> {
    let store = state.store()?;
    view_counts_impl(&store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::Address;
    use mailstore::{NewMessage, NewTask};

    /// アカウント + INBOX フォルダを作る。
    fn seed_account(store: &Store, email: &str, project_tag: Option<&str>) -> i64 {
        store
            .add_account(
                email,
                mailcore::AccountKind::Imap,
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
            has_attachments: raw_path.is_some(),
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

    fn sample_eml_with_attachment(filename: &str) -> Vec<u8> {
        format!(
            "From: sender <sender@mail.example>\r\n\
             To: recipient <recipient@mail.example>\r\n\
             Subject: attachment test\r\n\
             Date: Wed, 06 Sep 2026 09:00:00 +0900\r\n\
             Message-ID: <msg-att@mail.example>\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/mixed; boundary=\"BOUNDARY\"\r\n\
             \r\n\
             --BOUNDARY\r\n\
             Content-Type: text/plain; charset=\"utf-8\"\r\n\
             \r\n\
             本文です。\r\n\
             --BOUNDARY\r\n\
             Content-Type: text/plain\r\n\
             Content-Disposition: attachment; filename=\"{filename}\"\r\n\
             Content-Transfer-Encoding: 7bit\r\n\
             \r\n\
             hello attachment\r\n\
             --BOUNDARY--\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn list_threads_impl_applies_the_filter() {
        let store = Store::open_in_memory().unwrap();
        let account_a = seed_account(&store, "a@mail.example", Some("案件A"));
        let account_b = seed_account(&store, "b@mail.example", Some("案件B"));
        let folder_a = store.ensure_folder(account_a, "INBOX", "inbox").unwrap();
        let folder_b = store.ensure_folder(account_b, "INBOX", "inbox").unwrap();

        let now = chrono::Utc::now();
        insert_msg(&store, account_a, folder_a, 1, "t1", "件名1", now, None);
        let unread_id = insert_msg(&store, account_a, folder_a, 2, "t2", "件名2", now, None);
        insert_msg(&store, account_b, folder_b, 1, "t3", "件名3", now, None);
        store.set_read(&[unread_id], false).unwrap();

        // すべて
        let all = list_threads_impl(&store, ThreadFilter::default()).unwrap();
        assert_eq!(all.len(), 3);

        // project_tag
        let filtered = list_threads_impl(
            &store,
            ThreadFilter {
                project_tag: Some("案件A".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.len(), 2);

        // unread_only（seed した全メッセージは is_read=false なので全件残る）
        let unread = list_threads_impl(
            &store,
            ThreadFilter {
                unread_only: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(unread.len(), 3);

        // limit
        let limited = list_threads_impl(
            &store,
            ThreadFilter {
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn get_thread_impl_returns_messages_in_date_order() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", Some("案件A"));
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();

        let t0 = chrono::Utc::now() - chrono::Duration::hours(1);
        let t1 = chrono::Utc::now();
        insert_msg(&store, account, folder, 2, "thread-1", "Re: 件名", t1, None);
        insert_msg(&store, account, folder, 1, "thread-1", "件名", t0, None);

        let detail = get_thread_impl(&store, "thread-1").unwrap();
        assert_eq!(detail.messages.len(), 2);
        assert_eq!(detail.messages[0].date, t0.to_rfc3339());
        assert_eq!(detail.messages[1].date, t1.to_rfc3339());
        assert_eq!(detail.subject, "Re: 件名");
        assert_eq!(detail.project_tag.as_deref(), Some("案件A"));
    }

    #[test]
    fn get_thread_impl_reports_not_found_for_an_unknown_key() {
        let store = Store::open_in_memory().unwrap();
        let err = get_thread_impl(&store, "no-such-thread").unwrap_err();
        assert_eq!(err.code, "not_found");
    }

    #[test]
    fn get_thread_impl_has_no_summary_or_tasks_yet() {
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
            None,
        );

        let detail = get_thread_impl(&store, "thread-1").unwrap();
        assert!(detail.summary.is_none());
        assert!(detail.tasks.is_empty());
    }

    #[test]
    fn get_thread_impl_leaves_quoted_text_empty_when_the_eml_is_missing() {
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
            None,
        );

        let detail = get_thread_impl(&store, "thread-1").unwrap();
        assert_eq!(detail.messages[0].quoted_text, "");
    }

    #[test]
    fn get_thread_impl_fills_quoted_text_from_the_eml() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        fs::write(&eml_path, sample_eml_with_quote()).unwrap();

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

        let detail = get_thread_impl(&store, "thread-1").unwrap();
        assert!(detail.messages[0]
            .quoted_text
            .contains("元のメールの引用行"));
    }

    #[test]
    fn extract_attachment_impl_writes_the_file_and_records_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        fs::write(&eml_path, sample_eml_with_attachment("note.txt")).unwrap();
        let attachments_dir = dir.path().join("attachments");

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let message_id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "添付テスト",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );
        let attachment_id = store
            .insert_attachment_meta(message_id, "note.txt", "text/plain", 17)
            .unwrap();

        let path = extract_attachment_impl(&store, &attachments_dir, attachment_id).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello attachment");

        let recorded = store.get_attachment(attachment_id).unwrap().unwrap();
        assert_eq!(recorded.path.as_deref(), Some(path.as_str()));
    }

    #[test]
    fn extract_attachment_impl_reuses_the_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        fs::write(&eml_path, sample_eml_with_attachment("note.txt")).unwrap();
        let attachments_dir = dir.path().join("attachments");

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let message_id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "添付テスト",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );
        let attachment_id = store
            .insert_attachment_meta(message_id, "note.txt", "text/plain", 17)
            .unwrap();

        let first = extract_attachment_impl(&store, &attachments_dir, attachment_id).unwrap();
        // .eml を消しても、既に展開済みのファイルがあれば読み直さずそのパスを返す。
        fs::remove_file(&eml_path).unwrap();
        let second = extract_attachment_impl(&store, &attachments_dir, attachment_id).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn extract_attachment_impl_sanitises_the_filename() {
        let dir = tempfile::tempdir().unwrap();
        let eml_path = dir.path().join("1.eml");
        fs::write(&eml_path, sample_eml_with_attachment("../evil.txt")).unwrap();
        let attachments_dir = dir.path().join("attachments");

        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let message_id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "添付テスト",
            chrono::Utc::now(),
            Some(eml_path.to_str().unwrap()),
        );
        let attachment_id = store
            .insert_attachment_meta(message_id, "../evil.txt", "text/plain", 17)
            .unwrap();

        let path = extract_attachment_impl(&store, &attachments_dir, attachment_id).unwrap();
        let path = std::path::Path::new(&path);
        assert!(path.starts_with(&attachments_dir));
        assert!(path
            .canonicalize()
            .unwrap()
            .starts_with(attachments_dir.canonicalize().unwrap()));
    }

    #[test]
    fn mark_impl_rejects_an_unknown_action() {
        let store = Store::open_in_memory().unwrap();
        let err = mark_impl(&store, &[1], "explode").unwrap_err();
        assert_eq!(err.code, "invalid_input");
    }

    #[test]
    fn mark_impl_updates_read_and_archive() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", None);
        let folder = store.ensure_folder(account, "INBOX", "inbox").unwrap();
        let id = insert_msg(
            &store,
            account,
            folder,
            1,
            "thread-1",
            "件名",
            chrono::Utc::now(),
            None,
        );

        let changed = mark_impl(&store, &[id], "read").unwrap();
        assert_eq!(changed, 1);
        assert!(store.get_message(id).unwrap().unwrap().is_read);

        let changed = mark_impl(&store, &[id], "archive").unwrap();
        assert_eq!(changed, 1);

        // アーカイブ済みは include_archived なしの一覧に出てこない。
        let visible = list_threads_impl(&store, ThreadFilter::default()).unwrap();
        assert!(visible.is_empty());
    }

    #[test]
    fn get_digest_impl_rejects_a_bad_timestamp() {
        let store = Store::open_in_memory().unwrap();
        let err = get_digest_impl(&store, "not-a-timestamp", "2026-09-06").unwrap_err();
        assert_eq!(err.code, "invalid_input");
    }

    #[test]
    fn get_digest_impl_is_empty_without_tasks() {
        let store = Store::open_in_memory().unwrap();
        let digest = get_digest_impl(&store, "2026-09-06T00:00:00+09:00", "2026-09-06").unwrap();
        assert!(digest.summary.is_none());
        assert!(digest.groups.is_empty());
    }

    #[test]
    fn get_digest_impl_groups_tasks_by_project_tag() {
        let store = Store::open_in_memory().unwrap();
        let account = seed_account(&store, "a@mail.example", Some("案件A"));
        store
            .insert_task(&NewTask {
                account_id: account,
                source_message_id: None,
                title: "タスク1",
                due: None,
                confidence: 1.0,
                created_by: "ai",
            })
            .unwrap();

        let digest = get_digest_impl(&store, "2026-09-06T00:00:00+09:00", "2026-09-06").unwrap();
        assert_eq!(digest.groups.len(), 1);
        assert_eq!(digest.groups[0].project_tag.as_deref(), Some("案件A"));
        assert_eq!(digest.groups[0].tasks.len(), 1);
    }
}
