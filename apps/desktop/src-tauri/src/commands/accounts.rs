//! アカウント関連のコマンド。ロジックは `&Store` を取る素の関数に書き、
//! `#[tauri::command]` は薄いラッパにする。

use mailcore::MailBackend;
use mailstore::Store;

use crate::error::AppError;
use crate::state::AppState;

/// ウィザードから来るアカウント設定。パスワードは含まない（別コマンドで keyring に入れる）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NewAccountInput {
    pub name: String,
    pub kind: mailcore::AccountKind,
    pub email: String,
    pub project_tag: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub starttls: bool,
}

/// 接続テストの結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestConnectionResult {
    /// 見えたフォルダ名（先頭 20 件まで）。UI で「INBOX が見えた」ことを示すのに使う。
    pub folders: Vec<String>,
}

/// サイドバーの案件グループ。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectGroup {
    /// project_tag。未設定のアカウントは null のグループにまとまる。
    pub tag: Option<String>,
    pub accounts: Vec<ProjectAccount>,
    pub unread: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectAccount {
    pub id: i64,
    pub email: String,
}

/// `add_account` / `test_connection` で共有する入力チェック。
/// host / username が空、port が 0 なら弾く。
fn validate_new_account_input(input: &NewAccountInput) -> Result<(), AppError> {
    if input.kind != mailcore::AccountKind::Imap {
        return Err(AppError::invalid_input(
            "Gmail / Microsoft 365 はまだ対応していません",
        ));
    }
    if input.name.trim().is_empty() {
        return Err(AppError::invalid_input("アカウント名を入力してください"));
    }
    if input.email.trim().is_empty() {
        return Err(AppError::invalid_input("メールアドレスを入力してください"));
    }
    if input.host.trim().is_empty() {
        return Err(AppError::invalid_input("ホスト名を入力してください"));
    }
    if input.username.trim().is_empty() {
        return Err(AppError::invalid_input("ユーザー名を入力してください"));
    }
    if input.port == 0 {
        return Err(AppError::invalid_input("ポート番号を入力してください"));
    }
    Ok(())
}

/// `add_account` の本体。パスワードを含まない `settings_json` を組み立てて保存する。
fn add_account_impl(store: &Store, input: NewAccountInput) -> Result<mailcore::Account, AppError> {
    validate_new_account_input(&input)?;

    let email = input.email.trim();
    if store.list_accounts()?.iter().any(|a| a.email == email) {
        return Err(AppError::conflict(
            "このメールアドレスはすでに登録されています",
        ));
    }

    let settings = serde_json::json!({
        "host": input.host.trim(),
        "port": input.port,
        "username": input.username.trim(),
        "starttls": input.starttls,
    });

    store
        .add_account(
            input.name.trim(),
            input.kind,
            email,
            input.project_tag.as_deref(),
            &settings,
        )
        .map_err(AppError::from)
}

/// `delete_account` の DB 部分。アカウントが無ければ `not_found`。
fn delete_account_impl(store: &Store, id: i64) -> Result<(), AppError> {
    if store.get_account(id)?.is_none() {
        return Err(AppError::not_found("指定されたアカウントが見つかりません"));
    }
    store.delete_account(id)?;
    Ok(())
}

/// `list_projects` の組み立て本体。`project_tag` でグループ化し、
/// グループ内のアカウントの未読を合計する。並びは tag 昇順、`None` は最後。
fn list_projects_impl(store: &Store) -> Result<Vec<ProjectGroup>, AppError> {
    let accounts = store.list_accounts()?;
    let unread_map: std::collections::HashMap<i64, i64> =
        store.unread_counts_by_account()?.into_iter().collect();

    let mut tagged: std::collections::BTreeMap<String, Vec<ProjectAccount>> =
        std::collections::BTreeMap::new();
    let mut untagged: Vec<ProjectAccount> = Vec::new();

    for acc in &accounts {
        let pa = ProjectAccount {
            id: acc.id,
            email: acc.email.clone(),
        };
        match &acc.project_tag {
            Some(tag) => tagged.entry(tag.clone()).or_default().push(pa),
            None => untagged.push(pa),
        }
    }

    let sum_unread = |accs: &[ProjectAccount]| -> i64 {
        accs.iter()
            .map(|a| unread_map.get(&a.id).copied().unwrap_or(0))
            .sum()
    };

    let mut groups: Vec<ProjectGroup> = tagged
        .into_iter()
        .map(|(tag, accs)| {
            let unread = sum_unread(&accs);
            ProjectGroup {
                tag: Some(tag),
                accounts: accs,
                unread,
            }
        })
        .collect();

    if !untagged.is_empty() {
        let unread = sum_unread(&untagged);
        groups.push(ProjectGroup {
            tag: None,
            accounts: untagged,
            unread,
        });
    }

    Ok(groups)
}

#[tauri::command]
pub fn list_accounts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<mailcore::Account>, AppError> {
    let store = state.store()?;
    store.list_accounts().map_err(AppError::from)
}

#[tauri::command]
pub fn add_account(
    state: tauri::State<'_, AppState>,
    input: NewAccountInput,
) -> Result<mailcore::Account, AppError> {
    let store = state.store()?;
    add_account_impl(&store, input)
}

#[tauri::command]
pub fn set_account_password(id: i64, password: String) -> Result<(), AppError> {
    mailsync::imap::save_password(id, &password).map_err(AppError::from_backend)
}

#[tauri::command]
pub async fn test_connection(
    input: NewAccountInput,
    password: String,
) -> Result<TestConnectionResult, AppError> {
    validate_new_account_input(&input)?;

    let config = mailsync::imap::ImapConfig {
        account_id: 0,
        host: input.host.trim().to_string(),
        port: input.port,
        username: input.username.trim().to_string(),
        starttls: input.starttls,
    };
    let backend = mailsync::imap::ImapBackend::with_password(config, password);
    let folders = backend
        .list_folders()
        .await
        .map_err(AppError::from_backend)?;

    Ok(TestConnectionResult {
        folders: folders.into_iter().take(20).map(|(name, _)| name).collect(),
    })
}

#[tauri::command]
pub fn delete_account(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    {
        let store = state.store()?;
        delete_account_impl(&store, id)?;
    }

    // keyring にエントリが無いだけのこともあるので、失敗しても全体は失敗にしない。
    if let Err(e) = mailsync::imap::delete_password(id) {
        tracing::warn!(account_id = id, error = %e, "keyring からのパスワード削除に失敗しました");
    }

    let account_mail_dir = state.mail_dir().join(id.to_string());
    if account_mail_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&account_mail_dir) {
            tracing::warn!(account_id = id, error = %e, "raw .eml ディレクトリの削除に失敗しました");
        }
    }
    // TODO(P4): 削除したアカウントの添付キャッシュも掃除する

    Ok(())
}

#[tauri::command]
pub fn list_projects(state: tauri::State<'_, AppState>) -> Result<Vec<ProjectGroup>, AppError> {
    let store = state.store()?;
    list_projects_impl(&store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::{AccountKind, Address};
    use mailstore::NewMessage;

    fn valid_input() -> NewAccountInput {
        NewAccountInput {
            name: "client-a".into(),
            kind: AccountKind::Imap,
            email: "client-a@mail.example".into(),
            project_tag: Some("案件A".into()),
            host: "imap.mail.example".into(),
            port: 993,
            username: "client-a@mail.example".into(),
            starttls: false,
        }
    }

    #[test]
    fn add_account_rejects_gmail_and_m365() {
        let store = Store::open_in_memory().unwrap();

        let mut gmail = valid_input();
        gmail.kind = AccountKind::Gmail;
        let err = add_account_impl(&store, gmail).unwrap_err();
        assert_eq!(err.code, "invalid_input");

        let mut m365 = valid_input();
        m365.kind = AccountKind::M365;
        let err = add_account_impl(&store, m365).unwrap_err();
        assert_eq!(err.code, "invalid_input");
    }

    #[test]
    fn add_account_rejects_empty_fields() {
        let store = Store::open_in_memory().unwrap();

        let mut name_empty = valid_input();
        name_empty.name = "  ".into();
        assert_eq!(
            add_account_impl(&store, name_empty).unwrap_err().code,
            "invalid_input"
        );

        let mut host_empty = valid_input();
        host_empty.host = "".into();
        assert_eq!(
            add_account_impl(&store, host_empty).unwrap_err().code,
            "invalid_input"
        );

        let mut username_empty = valid_input();
        username_empty.username = "  ".into();
        assert_eq!(
            add_account_impl(&store, username_empty).unwrap_err().code,
            "invalid_input"
        );

        let mut port_zero = valid_input();
        port_zero.port = 0;
        assert_eq!(
            add_account_impl(&store, port_zero).unwrap_err().code,
            "invalid_input"
        );
    }

    #[test]
    fn add_account_stores_no_password_in_settings() {
        let store = Store::open_in_memory().unwrap();
        let account = add_account_impl(&store, valid_input()).unwrap();

        let obj = account.settings.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["host", "port", "starttls", "username"]);

        let dumped = account.settings.to_string();
        assert!(!dumped.to_lowercase().contains("password"));
    }

    #[test]
    fn add_account_then_list_accounts_returns_it() {
        let store = Store::open_in_memory().unwrap();
        let created = add_account_impl(&store, valid_input()).unwrap();

        let listed = store.list_accounts().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
        assert_eq!(listed[0].email, "client-a@mail.example");
    }

    #[test]
    fn add_account_rejects_a_duplicate_email() {
        let store = Store::open_in_memory().unwrap();
        add_account_impl(&store, valid_input()).unwrap();

        let mut dup = valid_input();
        dup.name = "client-a-2".into();
        let err = add_account_impl(&store, dup).unwrap_err();
        assert_eq!(err.code, "conflict");
    }

    fn seed_account_with_unread(
        store: &Store,
        email: &str,
        project_tag: Option<&str>,
        unread_count: usize,
    ) -> i64 {
        let account = store
            .add_account(
                email,
                AccountKind::Imap,
                email,
                project_tag,
                &serde_json::json!({}),
            )
            .unwrap();
        let folder = store.ensure_folder(account.id, "INBOX", "inbox").unwrap();
        for uid in 0..unread_count {
            let from = Address {
                name: None,
                email: "sender@mail.example".into(),
            };
            let m = NewMessage {
                account_id: account.id,
                folder_id: folder,
                uid: uid as u32 + 1,
                message_id: None,
                thread_key: &format!("t{uid}"),
                from: &from,
                to: &[],
                cc: &[],
                subject: "件名",
                date: chrono::Utc::now(),
                snippet: "件名",
                body_text: "本文",
                body_html: None,
                has_attachments: false,
                is_read: false,
                is_flagged: false,
                raw_path: None,
            };
            store.insert_message(&m).unwrap();
        }
        account.id
    }

    #[test]
    fn list_projects_groups_accounts_by_tag_and_sums_unread() {
        let store = Store::open_in_memory().unwrap();
        seed_account_with_unread(&store, "a1@mail.example", Some("案件A"), 2);
        seed_account_with_unread(&store, "a2@mail.example", Some("案件A"), 1);
        seed_account_with_unread(&store, "b1@mail.example", Some("案件B"), 3);

        let groups = list_projects_impl(&store).unwrap();
        assert_eq!(groups.len(), 2);

        let a = groups
            .iter()
            .find(|g| g.tag.as_deref() == Some("案件A"))
            .unwrap();
        assert_eq!(a.accounts.len(), 2);
        assert_eq!(a.unread, 3);

        let b = groups
            .iter()
            .find(|g| g.tag.as_deref() == Some("案件B"))
            .unwrap();
        assert_eq!(b.accounts.len(), 1);
        assert_eq!(b.unread, 3);
    }

    #[test]
    fn list_projects_puts_untagged_accounts_last() {
        let store = Store::open_in_memory().unwrap();
        seed_account_with_unread(&store, "untagged@mail.example", None, 1);
        seed_account_with_unread(&store, "z@mail.example", Some("案件Z"), 1);
        seed_account_with_unread(&store, "a@mail.example", Some("案件A"), 1);

        let groups = list_projects_impl(&store).unwrap();
        let tags: Vec<Option<String>> = groups.iter().map(|g| g.tag.clone()).collect();
        assert_eq!(
            tags,
            vec![Some("案件A".to_string()), Some("案件Z".to_string()), None]
        );
    }

    #[test]
    fn delete_account_impl_reports_not_found() {
        let store = Store::open_in_memory().unwrap();
        let err = delete_account_impl(&store, 9999).unwrap_err();
        assert_eq!(err.code, "not_found");
    }

    // set_account_password / test_connection / delete_account の keyring 経由部分は
    // OS の資格情報ストアとネットワークを触るため CI では実行できない。テストしない
    // （mailsync::imap::save_password / delete_password のコメントと同じ理由）。
}
