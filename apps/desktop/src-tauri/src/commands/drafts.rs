//! 返信下書きのコマンド。ロジックは `&Store` を取る素の関数に書き、
//! `#[tauri::command]` は薄いラッパにする。
//!
//! **送信はここに出さない。** 作れるのは下書き（status = "draft"）まで。
//! 送信は UI の承認操作だけが行う（MCP にも送信ツールを出さない）。

use mailstore::{NewDraft, Store};

use crate::error::AppError;
use crate::state::AppState;

/// 返信下書きの作成入力。返信元のメッセージから宛先・件名を決める。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NewDraftInput {
    /// 返信元のメッセージ id。これが下書きのアカウント・宛先・件名の元になる。
    pub in_reply_to: i64,
    pub body: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DraftDto {
    pub id: i64,
    pub account_id: i64,
    pub in_reply_to: Option<i64>,
    pub to: Vec<mailcore::Address>,
    pub subject: String,
    pub body: String,
    /// 常に "draft"。送信は UI の承認操作だけ（MCP にも送信ツールを出さない）。
    pub status: String,
    pub created_at: String,
}

fn draft_to_dto(d: mailcore::Draft) -> DraftDto {
    DraftDto {
        id: d.id,
        account_id: d.account_id,
        in_reply_to: d.in_reply_to,
        to: d.to,
        subject: d.subject,
        body: d.body,
        status: d.status,
        created_at: d.created_at.to_rfc3339(),
    }
}

/// 件名に `Re: ` を付ける。既に（大文字小文字を無視して）`re:` で始まっていれば
/// そのまま返す。`mailcore::normalize_subject` は接頭辞を全部剥がすものなので、
/// ここでは「先頭が re: かどうか」だけを自前で見る。
fn with_reply_prefix(subject: &str) -> String {
    let already = subject
        .trim_start()
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("re:"));
    if already {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

/// `create_draft` の本体。
fn create_draft_impl(store: &Store, input: NewDraftInput) -> Result<DraftDto, AppError> {
    if input.body.trim().is_empty() {
        return Err(AppError::invalid_input("本文が空です"));
    }

    let source = store
        .get_message(input.in_reply_to)?
        .ok_or_else(|| AppError::not_found("返信元のメールが見つかりません"))?;

    // TODO(P3-b): 全員に返信（cc を含める）を選べるようにする
    let to = vec![source.from];
    let subject = with_reply_prefix(&source.subject);

    let id = store.insert_draft(&NewDraft {
        account_id: source.account_id,
        in_reply_to: Some(source.id),
        to: &to,
        subject: &subject,
        body: &input.body,
    })?;

    let draft = store
        .get_draft(id)?
        .ok_or_else(|| AppError::internal("下書きの保存に失敗しました"))?;
    Ok(draft_to_dto(draft))
}

/// `list_drafts` の本体。
fn list_drafts_impl(store: &Store) -> Result<Vec<DraftDto>, AppError> {
    let drafts = store.list_drafts(0)?;
    Ok(drafts.into_iter().map(draft_to_dto).collect())
}

#[tauri::command]
pub fn create_draft(
    state: tauri::State<'_, AppState>,
    input: NewDraftInput,
) -> Result<DraftDto, AppError> {
    let store = state.store()?;
    create_draft_impl(&store, input)
}

#[tauri::command]
pub fn list_drafts(state: tauri::State<'_, AppState>) -> Result<Vec<DraftDto>, AppError> {
    let store = state.store()?;
    list_drafts_impl(&store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::Address;
    use mailstore::NewMessage;

    /// 呼び出しごとに増える値。`seed_message` がテスト内で複数アカウントを
    /// 作るときに一意なメールアドレスを組み立てるのに使う（`accounts.email` は UNIQUE）。
    fn next_seq() -> usize {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        SEQ.fetch_add(1, Ordering::Relaxed)
    }

    fn seed_message(store: &Store, subject: &str) -> (i64, i64) {
        let email = format!("test-{}@mail.example", next_seq());
        let account = store
            .add_account(
                "test",
                mailcore::AccountKind::Imap,
                &email,
                None,
                &serde_json::json!({}),
            )
            .unwrap();
        let folder = store.ensure_folder(account.id, "INBOX", "inbox").unwrap();
        let from = Address {
            name: Some("山田".into()),
            email: "yamada@client-a.example".into(),
        };
        let message_id = store
            .insert_message(&NewMessage {
                account_id: account.id,
                folder_id: folder,
                uid: 1,
                message_id: None,
                thread_key: "t1",
                from: &from,
                to: &[],
                cc: &[],
                subject,
                date: chrono::Utc::now(),
                snippet: subject,
                body_text: "本文です。",
                body_html: None,
                has_attachments: false,
                is_read: false,
                is_flagged: false,
                raw_path: None,
            })
            .unwrap()
            .unwrap();
        (account.id, message_id)
    }

    #[test]
    fn create_draft_impl_uses_the_sender_as_the_recipient() {
        let store = Store::open_in_memory().unwrap();
        let (account_id, message_id) = seed_message(&store, "見積の件");

        let dto = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: message_id,
                body: "ご連絡ありがとうございます。".into(),
            },
        )
        .unwrap();

        assert_eq!(dto.account_id, account_id);
        assert_eq!(dto.to.len(), 1);
        assert_eq!(dto.to[0].email, "yamada@client-a.example");
    }

    #[test]
    fn create_draft_impl_prefixes_the_subject_with_re_once() {
        let store = Store::open_in_memory().unwrap();
        let (_account_id, message_id) = seed_message(&store, "見積の件");

        let dto = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: message_id,
                body: "本文".into(),
            },
        )
        .unwrap();
        assert_eq!(dto.subject, "Re: 見積の件");

        let (_account_id2, already_re) = seed_message(&store, "Re: 見積の件");
        let dto2 = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: already_re,
                body: "本文".into(),
            },
        )
        .unwrap();
        assert_eq!(dto2.subject, "Re: 見積の件");
    }

    #[test]
    fn create_draft_impl_rejects_an_empty_body() {
        let store = Store::open_in_memory().unwrap();
        let (_account_id, message_id) = seed_message(&store, "見積の件");

        let err = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: message_id,
                body: "   ".into(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid_input");
    }

    #[test]
    fn create_draft_impl_reports_not_found_for_an_unknown_message() {
        let store = Store::open_in_memory().unwrap();
        let err = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: 9999,
                body: "本文".into(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "not_found");
    }

    #[test]
    fn create_draft_impl_saves_with_draft_status() {
        let store = Store::open_in_memory().unwrap();
        let (_account_id, message_id) = seed_message(&store, "見積の件");

        let dto = create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: message_id,
                body: "本文".into(),
            },
        )
        .unwrap();
        assert_eq!(dto.status, "draft");
    }

    #[test]
    fn list_drafts_impl_returns_what_was_created() {
        let store = Store::open_in_memory().unwrap();
        let (_account_id, message_id) = seed_message(&store, "見積の件");

        create_draft_impl(
            &store,
            NewDraftInput {
                in_reply_to: message_id,
                body: "本文".into(),
            },
        )
        .unwrap();

        let drafts = list_drafts_impl(&store).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].body, "本文");
    }
}
