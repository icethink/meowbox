//! 汎用 IMAP バックエンド。`async-imap`（`runtime-tokio`）で接続する。
//!
//! - 認証: `LOGIN`（PLAIN）。パスワードは `keyring` からのみ読む。DB にも設定ファイルにも置かない。
//! - TLS: `starttls == false` なら 993 相当の暗黙 TLS、`true` なら平文で繋いでから `STARTTLS`。
//!   どちらも最終的に `tokio-rustls` + `webpki-roots` の `TlsStream<TcpStream>` に揃うので、
//!   ログイン後のコードは 1 本で済む。
//!   （`async-imap` を `runtime-tokio` フィーチャで使うと、`tokio-rustls` が返すストリームは
//!   そのまま tokio の `AsyncRead`/`AsyncWrite` を実装しているため、`tokio-util` の `compat`
//!   による橋渡しは不要だった。デフォルトの `runtime-async-std` を使う場合は futures 系の
//!   トレイトが要求されるので必要になる）。
//! - `list_folders`: `LIST "" "*"` + SPECIAL-USE 属性、無ければ名前から推定する。
//! - `folder_status`: `EXAMINE`（読み取り専用）で UIDVALIDITY / UIDNEXT を取る。
//! - `fetch_new`: `UID SEARCH UID <since_uid+1>:* [SINCE <date>]` → 200 件ずつ
//!   `UID FETCH (UID FLAGS RFC822)`。本文が取れなかった UID は飛ばして続行する。
//!
//! 接続は毎回張り直す（コネクション再利用は P4）。

use std::sync::Arc;

use async_imap::types::{Flag, Mailbox, Name, NameAttribute};
use async_imap::{Client as ImapClient, Session as ImapSession};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mailcore::{BackendError, FolderRole, FolderStatus, MailBackend, RawMessage};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

/// `UID FETCH` を一度に投げる件数の上限。
const FETCH_CHUNK_SIZE: usize = 200;

#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub account_id: i64,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub starttls: bool,
}

/// アカウント設定から `ImapConfig` を組み立てられない理由。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("kind '{kind}' not supported yet")]
    NotImap { kind: &'static str },
    #[error("has no host configured")]
    NoHost,
}

impl ImapConfig {
    /// アカウントの `settings` から組み立てる。
    /// host は必須、port の既定は 993、username の既定は email、starttls の既定は false。
    pub fn from_account(a: &mailcore::Account) -> Result<Self, ConfigError> {
        if a.kind != mailcore::AccountKind::Imap {
            return Err(ConfigError::NotImap {
                kind: a.kind.as_str(),
            });
        }
        let host = a
            .settings
            .get("host")
            .and_then(|v| v.as_str())
            .ok_or(ConfigError::NoHost)?;
        let port = a
            .settings
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(993) as u16;
        let username = a
            .settings
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or(&a.email);
        let starttls = a
            .settings
            .get("starttls")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(ImapConfig {
            account_id: a.id,
            host: host.to_string(),
            port,
            username: username.to_string(),
            starttls,
        })
    }
}

pub struct ImapBackend {
    pub config: ImapConfig,
    /// `Some` のときは keyring を引かずにこのパスワードを使う（保存前の接続テスト用）。
    /// パスワードを Debug / tracing に出す経路を作らないこと。
    password: Option<String>,
}

impl ImapBackend {
    /// keyring からパスワードを読む通常の使い方。
    pub fn new(config: ImapConfig) -> Self {
        Self {
            config,
            password: None,
        }
    }

    /// パスワードを直接渡す。アカウントをまだ保存していない接続テストで使う。
    /// このとき `config.account_id` は keyring 参照に使われないので 0 でよい。
    pub fn with_password(config: ImapConfig, password: String) -> Self {
        Self {
            config,
            password: Some(password),
        }
    }

    /// TCP + TLS（暗黙 TLS or STARTTLS）で繋ぎ、ログインまで済ませたセッションを返す。
    /// 呼び出すたびに新しく接続する。
    async fn connect(&self) -> Result<ImapSession<TlsStream<TcpStream>>, BackendError> {
        let password = match &self.password {
            Some(p) => p.clone(),
            None => load_password(self.config.account_id)?,
        };

        tracing::debug!(
            host = %self.config.host,
            port = self.config.port,
            starttls = self.config.starttls,
            "imap: connecting"
        );

        let tcp = TcpStream::connect((self.config.host.as_str(), self.config.port))
            .await
            .map_err(|e| {
                BackendError::Network(format!(
                    "connect to {}:{} failed: {e}",
                    self.config.host, self.config.port
                ))
            })?;

        let connector = tls_connector();
        let server_name = ServerName::try_from(self.config.host.clone()).map_err(|e| {
            BackendError::Network(format!("invalid host name {}: {e}", self.config.host))
        })?;

        let session = if self.config.starttls {
            // 平文で繋いでから STARTTLS で TLS に昇格する。
            let mut client = ImapClient::new(tcp);
            client
                .read_response()
                .await
                .map_err(|e| BackendError::Network(e.to_string()))?;
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(protocol_err)?;
            let tcp = client.into_inner();

            let tls = connector.connect(server_name, tcp).await.map_err(|e| {
                BackendError::Network(format!("TLS handshake failed after STARTTLS: {e}"))
            })?;
            // STARTTLS 後はサーバーからの greeting は無い。
            let client = ImapClient::new(tls);
            client
                .login(&self.config.username, &password)
                .await
                .map_err(|(e, _client)| auth_err(e))?
        } else {
            // 993 相当の暗黙 TLS。
            let tls = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| BackendError::Network(format!("TLS handshake failed: {e}")))?;
            let mut client = ImapClient::new(tls);
            client
                .read_response()
                .await
                .map_err(|e| BackendError::Network(e.to_string()))?;
            client
                .login(&self.config.username, &password)
                .await
                .map_err(|(e, _client)| auth_err(e))?
        };

        tracing::debug!(host = %self.config.host, "imap: connected");
        Ok(session)
    }
}

/// keyring からパスワードを読む。DB にも設定ファイルにも置かない。
pub fn load_password(account_id: i64) -> Result<String, BackendError> {
    let entry = keyring::Entry::new("meowbox", &format!("account:{account_id}")).map_err(|e| {
        BackendError::Auth(format!(
            "keyring entry unavailable for account {account_id}: {e}"
        ))
    })?;
    entry.get_password().map_err(|e| {
        BackendError::Auth(format!(
            "failed to read password for account {account_id} from keyring: {e}"
        ))
    })
}

/// keyring にパスワードを保存する。DB にも設定ファイルにも書かない。
///
/// keyring は OS の資格情報ストアを触るため、この関数のテストは無い（CI では走らせられない）。
pub fn save_password(account_id: i64, password: &str) -> Result<(), BackendError> {
    let entry = keyring::Entry::new("meowbox", &format!("account:{account_id}")).map_err(|e| {
        BackendError::Auth(format!(
            "keyring entry unavailable for account {account_id}: {e}"
        ))
    })?;
    entry.set_password(password).map_err(|e| {
        BackendError::Auth(format!(
            "failed to save password for account {account_id} to keyring: {e}"
        ))
    })
}

/// keyring からパスワードを消す。エントリが無い場合は成功扱い。
///
/// keyring は OS の資格情報ストアを触るため、この関数のテストは無い（CI では走らせられない。
/// `save_password` と同じ理由）。
pub fn delete_password(account_id: i64) -> Result<(), BackendError> {
    let entry = keyring::Entry::new("meowbox", &format!("account:{account_id}")).map_err(|e| {
        BackendError::Auth(format!(
            "keyring entry unavailable for account {account_id}: {e}"
        ))
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(BackendError::Auth(format!(
            "failed to delete password for account {account_id} from keyring: {e}"
        ))),
    }
}

/// `webpki-roots` のルート証明書を使う TLS コネクタ。
fn tls_connector() -> TlsConnector {
    let root_store = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

/// ログイン以外の場面向けに `async_imap` のエラーを分類する。
fn protocol_err(e: async_imap::error::Error) -> BackendError {
    match e {
        async_imap::error::Error::Io(io_err) => BackendError::Network(io_err.to_string()),
        async_imap::error::Error::ConnectionLost => BackendError::Network("connection lost".into()),
        other => BackendError::Protocol(other.to_string()),
    }
}

/// ログイン失敗向けに `async_imap` のエラーを分類する。
fn auth_err(e: async_imap::error::Error) -> BackendError {
    match e {
        async_imap::error::Error::Io(io_err) => BackendError::Network(io_err.to_string()),
        async_imap::error::Error::ConnectionLost => BackendError::Network("connection lost".into()),
        other => BackendError::Auth(other.to_string()),
    }
}

/// SPECIAL-USE 属性から `FolderRole` を決める。`\Junk` は `FolderRole::Other` に読み替える。
/// 属性から判定できなければ `None`（呼び出し側で名前推定にフォールバックする）。
fn role_from_attributes(attributes: &[NameAttribute<'_>]) -> Option<FolderRole> {
    for attr in attributes {
        match attr {
            NameAttribute::Sent => return Some(FolderRole::Sent),
            NameAttribute::Drafts => return Some(FolderRole::Drafts),
            NameAttribute::Trash => return Some(FolderRole::Trash),
            NameAttribute::Archive => return Some(FolderRole::Archive),
            NameAttribute::Junk => return Some(FolderRole::Other),
            _ => {}
        }
    }
    None
}

/// SPECIAL-USE 属性が無いサーバー向けに、フォルダ名から役割を推定する（純粋関数）。
/// `INBOX` は常に `Inbox`。それ以外は階層区切り（`.` / `/`）付きの名前の最後の要素で判定する。
fn role_from_name(name: &str) -> FolderRole {
    let trimmed = name.trim();
    if trimmed.eq_ignore_ascii_case("INBOX") {
        return FolderRole::Inbox;
    }

    let last = trimmed.rsplit(['.', '/']).next().unwrap_or(trimmed).trim();
    // ASCII 英字だけを小文字化する。日本語部分はそのまま通る。
    let lower = last.to_ascii_lowercase();

    match lower.as_str() {
        "sent" | "sent items" | "sent messages" | "送信済み" | "送信済みトレイ" => {
            FolderRole::Sent
        }
        "drafts" | "下書き" | "草稿" => FolderRole::Drafts,
        "trash" | "deleted items" | "ゴミ箱" | "削除済み" => FolderRole::Trash,
        "archive" | "アーカイブ" => FolderRole::Archive,
        _ => FolderRole::Other,
    }
}

/// `SINCE` に使う IMAP の日付書式（例: `01-Sep-2025`）を作る。
fn imap_date(dt: DateTime<Utc>) -> String {
    dt.format("%d-%b-%Y").to_string()
}

/// `UID SEARCH` の検索条件を組み立てる。`since_uid` が `u32::MAX` でも溢れない。
fn build_search_query(since_uid: u32, since: Option<DateTime<Utc>>) -> String {
    let lower = since_uid.saturating_add(1);
    let mut query = format!("UID {lower}:*");
    if let Some(dt) = since {
        query.push_str(" SINCE ");
        query.push_str(&imap_date(dt));
    }
    query
}

/// `Flag` を IMAP の文字列表現に変換する。
fn flag_to_string(flag: Flag<'_>) -> String {
    match flag {
        Flag::Seen => "\\Seen".to_string(),
        Flag::Answered => "\\Answered".to_string(),
        Flag::Flagged => "\\Flagged".to_string(),
        Flag::Deleted => "\\Deleted".to_string(),
        Flag::Draft => "\\Draft".to_string(),
        Flag::Recent => "\\Recent".to_string(),
        Flag::MayCreate => "\\*".to_string(),
        Flag::Custom(s) => s.into_owned(),
    }
}

fn mailbox_status(mailbox: Mailbox, folder: &str) -> Result<FolderStatus, BackendError> {
    let uidvalidity = mailbox.uid_validity.ok_or_else(|| {
        BackendError::Protocol(format!("{folder}: server did not return UIDVALIDITY"))
    })?;
    let uid_next = mailbox.uid_next.ok_or_else(|| {
        BackendError::Protocol(format!("{folder}: server did not return UIDNEXT"))
    })?;
    Ok(FolderStatus {
        uidvalidity,
        uid_next,
    })
}

#[async_trait::async_trait]
impl MailBackend for ImapBackend {
    async fn list_folders(&self) -> Result<Vec<(String, FolderRole)>, BackendError> {
        let mut session = self.connect().await?;

        let names: Vec<Name> = session
            .list(None, Some("\"*\""))
            .await
            .map_err(protocol_err)?
            .try_collect()
            .await
            .map_err(protocol_err)?;

        let folders = names
            .iter()
            .map(|n| {
                let name = n.name().to_string();
                let role =
                    role_from_attributes(n.attributes()).unwrap_or_else(|| role_from_name(&name));
                (name, role)
            })
            .collect();

        let _ = session.logout().await;
        Ok(folders)
    }

    async fn folder_status(&self, folder: &str) -> Result<FolderStatus, BackendError> {
        let mut session = self.connect().await?;
        let mailbox = session.examine(folder).await.map_err(protocol_err)?;
        let status = mailbox_status(mailbox, folder);
        let _ = session.logout().await;
        status
    }

    async fn fetch_new(
        &self,
        folder: &str,
        since_uid: u32,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<RawMessage>, BackendError> {
        let mut session = self.connect().await?;
        session.examine(folder).await.map_err(protocol_err)?;

        let query = build_search_query(since_uid, since);
        let mut uids: Vec<u32> = session
            .uid_search(&query)
            .await
            .map_err(protocol_err)?
            .into_iter()
            .collect();
        uids.sort_unstable();

        let mut messages = Vec::new();
        for chunk in uids.chunks(FETCH_CHUNK_SIZE) {
            let set = chunk
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let mut stream = match session.uid_fetch(&set, "(UID FLAGS RFC822)").await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(folder, error = %e, "imap: uid_fetch failed for a chunk; skipping it");
                    continue;
                }
            };

            loop {
                match stream.try_next().await {
                    Ok(Some(f)) => match (f.uid, f.body()) {
                        (Some(uid), Some(body)) => {
                            let flags = f.flags().map(flag_to_string).collect();
                            messages.push(RawMessage {
                                uid,
                                flags,
                                raw: body.to_vec(),
                            });
                        }
                        _ => {
                            tracing::warn!(
                                folder,
                                "imap: fetch response missing uid or body; skipping it"
                            );
                        }
                    },
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(folder, error = %e, "imap: fetch stream error; skipping rest of this chunk");
                        break;
                    }
                }
            }
        }

        tracing::debug!(folder, fetched = messages.len(), "imap: fetch_new done");
        let _ = session.logout().await;
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn role_from_name_maps_inbox_regardless_of_case() {
        assert_eq!(role_from_name("INBOX"), FolderRole::Inbox);
        assert_eq!(role_from_name("inbox"), FolderRole::Inbox);
    }

    #[test]
    fn role_from_name_maps_english_names() {
        assert_eq!(role_from_name("Sent"), FolderRole::Sent);
        assert_eq!(role_from_name("Sent Items"), FolderRole::Sent);
        assert_eq!(role_from_name("Sent Messages"), FolderRole::Sent);
        assert_eq!(role_from_name("Drafts"), FolderRole::Drafts);
        assert_eq!(role_from_name("Trash"), FolderRole::Trash);
        assert_eq!(role_from_name("Deleted Items"), FolderRole::Trash);
        assert_eq!(role_from_name("Archive"), FolderRole::Archive);
    }

    #[test]
    fn role_from_name_maps_japanese_names() {
        assert_eq!(role_from_name("送信済み"), FolderRole::Sent);
        assert_eq!(role_from_name("送信済みトレイ"), FolderRole::Sent);
        assert_eq!(role_from_name("下書き"), FolderRole::Drafts);
        assert_eq!(role_from_name("草稿"), FolderRole::Drafts);
        assert_eq!(role_from_name("ゴミ箱"), FolderRole::Trash);
        assert_eq!(role_from_name("削除済み"), FolderRole::Trash);
        assert_eq!(role_from_name("アーカイブ"), FolderRole::Archive);
    }

    #[test]
    fn role_from_name_uses_last_segment_of_hierarchical_names() {
        assert_eq!(role_from_name("INBOX.送信済み"), FolderRole::Sent);
        assert_eq!(role_from_name("INBOX/Sent Items"), FolderRole::Sent);
        assert_eq!(role_from_name("Team.Projects.Archive"), FolderRole::Archive);
    }

    #[test]
    fn role_from_name_falls_back_to_other_for_unknown_names() {
        assert_eq!(role_from_name("Projects"), FolderRole::Other);
        assert_eq!(role_from_name("案件A"), FolderRole::Other);
    }

    #[test]
    fn imap_date_uses_english_three_letter_months() {
        let dt = Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap();
        assert_eq!(imap_date(dt), "01-Sep-2025");

        let jan = Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap();
        assert_eq!(imap_date(jan), "05-Jan-2025");

        let dec = Utc.with_ymd_and_hms(2025, 12, 31, 0, 0, 0).unwrap();
        assert_eq!(imap_date(dec), "31-Dec-2025");
    }

    #[test]
    fn build_search_query_includes_since_when_present() {
        let dt = Utc.with_ymd_and_hms(2025, 6, 6, 0, 0, 0).unwrap();
        assert_eq!(
            build_search_query(100, Some(dt)),
            "UID 101:* SINCE 06-Jun-2025"
        );
        assert_eq!(build_search_query(100, None), "UID 101:*");
    }

    #[test]
    fn build_search_query_does_not_overflow_at_u32_max() {
        assert_eq!(
            build_search_query(u32::MAX, None),
            format!("UID {}:*", u32::MAX)
        );
    }
}
