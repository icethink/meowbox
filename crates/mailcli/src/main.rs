//! `meowbox` CLI。UI も MCP も無しで同期・検索ができるデバッグ入口。
//!
//!   meowbox init
//!   meowbox accounts add --name work --kind imap --email me@example.com --project 案件A \
//!       --host imap.example.com --port 993 --username me@example.com
//!   meowbox accounts set-password 1
//!   meowbox accounts list
//!   meowbox sync --account 1 --folder INBOX
//!   meowbox search "見積" --project 案件A --unread
//!   meowbox show 42
//!
//! DB の場所は `--db` か `MEOWBOX_DB`、既定は `./data/meowbox.db`。

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use mailcore::AccountKind;
use mailstore::{SearchQuery, Store};
use mailsync::engine::{SyncEngine, SyncOptions};
use mailsync::imap::{save_password, ImapBackend, ImapConfig};

#[derive(Parser)]
#[command(
    name = "meowbox",
    version,
    about = "Meowbox — AI-friendly mail aggregator"
)]
struct Cli {
    /// SQLite DB のパス
    #[arg(long, env = "MEOWBOX_DB", default_value = "data/meowbox.db")]
    db: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// DB を作成しスキーマを適用する
    Init,
    /// アカウント管理
    Accounts {
        #[command(subcommand)]
        cmd: AccountsCmd,
    },
    /// 同期を 1 回実行する
    Sync {
        /// 省略時は全 IMAP アカウントを順に同期する
        #[arg(long)]
        account: Option<i64>,
        /// 省略時は全フォルダを同期する
        #[arg(long)]
        folder: Option<String>,
        /// この日数より新しいメールだけ取る
        #[arg(long, default_value_t = 90)]
        days: i64,
    },
    /// 全文検索
    Search {
        query: Option<String>,
        #[arg(long)]
        account: Option<i64>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        unread: bool,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// JSON で出力（Claude に食わせる用）
        #[arg(long)]
        json: bool,
    },
    /// 1 通を本文つきで表示する
    Show {
        id: i64,
        /// JSON で出力（Claude に食わせる用）
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AccountsCmd {
    List,
    Add {
        #[arg(long)]
        name: String,
        /// imap | gmail | m365
        #[arg(long, default_value = "imap")]
        kind: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        host: Option<String>,
        #[arg(long, default_value_t = 993)]
        port: u16,
        /// 省略時は --email の値を使う
        #[arg(long)]
        username: Option<String>,
        /// STARTTLS を使う（既定は暗黙 TLS）
        #[arg(long, default_value_t = false)]
        starttls: bool,
    },
    /// アカウントのパスワードを keyring に保存する（非表示入力）
    SetPassword {
        id: i64,
    },
}

/// アカウントを同期できない理由。
#[derive(Debug, PartialEq, Eq)]
enum SkipReason {
    /// `kind` が imap ではない（gmail / m365 は未対応）。
    NotImap { kind: &'static str },
    /// imap アカウントだが `settings.host` が無い。
    NoHost,
}

impl SkipReason {
    fn message(&self, account_id: mailcore::AccountId) -> String {
        match self {
            SkipReason::NotImap { kind } => {
                format!("account #{account_id}: kind '{kind}' not supported yet")
            }
            SkipReason::NoHost => format!("account #{account_id}: has no host configured"),
        }
    }
}

/// アカウント設定から `ImapConfig` を組み立てる。同期できないアカウントは
/// 飛ばす理由を返す。
fn imap_config_from_account(a: &mailcore::Account) -> Result<ImapConfig, SkipReason> {
    if a.kind != AccountKind::Imap {
        return Err(SkipReason::NotImap {
            kind: a.kind.as_str(),
        });
    }
    let host = a
        .settings
        .get("host")
        .and_then(|v| v.as_str())
        .ok_or(SkipReason::NoHost)?;
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

/// `sync` サブコマンド 1 回分の集計（試行数・失敗数・スキップ数）。
#[derive(Debug, Default, PartialEq, Eq)]
struct SyncOutcome {
    /// 同期を実際に試みたアカウント数（設定不備でのスキップは含まない）。
    attempted: usize,
    /// `attempted` のうち同期に失敗した数。
    failed: usize,
    /// host 未設定・非 imap で同期対象外としたアカウント数。
    skipped: usize,
}

/// 集計から `sync` の終了結果を決める。
/// - `Ok(Some(summary))`: 成功。複数アカウントを回った場合はサマリ文字列付き。
/// - `Ok(None)`: 成功。1 アカウントのみ（追加のサマリ行は出さない）。
/// - `Err(message)`: 非ゼロ終了すべき理由。
fn sync_exit_result(o: &SyncOutcome) -> Result<Option<String>, String> {
    if o.attempted == 0 {
        return Err(if o.skipped > 0 {
            "no accounts to sync (all accounts were skipped — check --account id, kind, host)"
                .to_string()
        } else {
            "no accounts to sync".to_string()
        });
    }
    if o.failed > 0 {
        return Err(format!(
            "sync failed for {} of {} account(s)",
            o.failed, o.attempted
        ));
    }
    if o.attempted > 1 {
        Ok(Some(format!(
            "synced {} account(s), {} failed",
            o.attempted, o.failed
        )))
    } else {
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    if let Some(parent) = cli.db.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let store = Store::open(&cli.db).with_context(|| format!("open {}", cli.db.display()))?;

    match cli.cmd {
        Cmd::Init => {
            println!("initialized {}", cli.db.display());
        }
        Cmd::Accounts { cmd } => match cmd {
            AccountsCmd::List => {
                let accounts = store.list_accounts()?;
                if accounts.is_empty() {
                    println!("(no accounts) — try: meowbox accounts add --help");
                }
                for a in accounts {
                    println!(
                        "{:>3}  {:<6} {:<32} {:<12} {}",
                        a.id,
                        a.kind.as_str(),
                        a.email,
                        a.project_tag.as_deref().unwrap_or("-"),
                        a.name
                    );
                }
            }
            AccountsCmd::Add {
                name,
                kind,
                email,
                project,
                host,
                port,
                username,
                starttls,
            } => {
                let kind = AccountKind::parse(&kind)
                    .with_context(|| format!("unknown kind '{kind}' (imap|gmail|m365)"))?;
                let username = username.unwrap_or_else(|| email.clone());
                let settings = serde_json::json!({
                    "host": host,
                    "port": port,
                    "username": username,
                    "starttls": starttls,
                });
                let a = store.add_account(&name, kind, &email, project.as_deref(), &settings)?;
                println!("added account #{} {}", a.id, a.email);
                println!(
                    "note: パスワードは保存されていません。`meowbox accounts set-password {}` で設定してください",
                    a.id
                );
            }
            AccountsCmd::SetPassword { id } => {
                let accounts = store.list_accounts()?;
                if !accounts.iter().any(|a| a.id == id) {
                    anyhow::bail!("account #{id} not found (see: meowbox accounts list)");
                }
                let password =
                    rpassword::prompt_password("password: ").context("failed to read password")?;
                if password.is_empty() {
                    anyhow::bail!("password must not be empty");
                }
                save_password(id, &password).context("failed to save password to keyring")?;
                println!("saved password for account #{id}");
            }
        },
        Cmd::Sync {
            account,
            folder,
            days,
        } => {
            let accounts = store.list_accounts()?;
            let targets: Vec<_> = match account {
                Some(id) => accounts.into_iter().filter(|a| a.id == id).collect(),
                None => accounts,
            };

            // `SyncEngine::store` は `Arc<Store>` 固定（呼び出し側の要求）。CLI は単一
            // スレッドで順に同期するだけなので false positive。
            #[allow(clippy::arc_with_non_send_sync)]
            let store = Arc::new(store);
            let engine = SyncEngine::new(store.clone());
            let since = Utc::now() - Duration::days(days);

            let mut outcome = SyncOutcome::default();

            for a in targets {
                let config = match imap_config_from_account(&a) {
                    Ok(config) => config,
                    Err(reason) => {
                        eprintln!("{}", reason.message(a.id));
                        outcome.skipped += 1;
                        continue;
                    }
                };

                let backend = ImapBackend::new(config);
                let opts = SyncOptions {
                    only_folder: folder.clone(),
                    since: Some(since),
                    data_dir: PathBuf::from("data/mail"),
                };

                outcome.attempted += 1;
                match engine.sync_once(a.id, &backend, &opts).await {
                    Ok(report) => println!(
                        "account #{}: fetched={} inserted={} skipped={} errors={}",
                        a.id, report.fetched, report.inserted, report.skipped, report.errors
                    ),
                    Err(e) => {
                        eprintln!("account #{}: sync failed: {e}", a.id);
                        outcome.failed += 1;
                    }
                }
            }

            match sync_exit_result(&outcome) {
                Ok(Some(summary)) => println!("{summary}"),
                Ok(None) => {}
                Err(message) => anyhow::bail!(message),
            }
        }
        Cmd::Search {
            query,
            account,
            project,
            unread,
            limit,
            json,
        } => {
            let hits = store.search(&SearchQuery {
                text: query.as_deref(),
                account_id: account,
                project_tag: project.as_deref(),
                since: None,
                unread_only: unread,
                limit,
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&hits)?);
            } else {
                for h in hits {
                    println!(
                        "{} {:>5} {} {:<28} {}",
                        if h.is_read { " " } else { "*" },
                        h.id,
                        h.date.format("%Y-%m-%d %H:%M"),
                        h.from.name.as_deref().unwrap_or(&h.from.email),
                        h.subject
                    );
                }
            }
        }
        Cmd::Show { id, json } => {
            let Some(m) = store.get_message(id)? else {
                anyhow::bail!("message {id} not found");
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&m)?);
            } else {
                println!("Subject: {}", m.subject);
                println!("From: {}", m.from.name.as_deref().unwrap_or(&m.from.email));
                let to =
                    m.to.iter()
                        .map(|a| a.name.as_deref().unwrap_or(&a.email))
                        .collect::<Vec<_>>()
                        .join(", ");
                println!("To: {to}");
                println!("Date: {}", m.date.format("%Y-%m-%d %H:%M"));
                println!("Folder: {}", m.folder_path);
                println!("UID: {}", m.uid);
                println!();
                println!("{}", m.body_text);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcore::Account;

    /// テスト用の `Account` を組み立てる。アドレスは RFC 2606 の `.example` のみ使う。
    fn account(kind: AccountKind, settings: serde_json::Value) -> Account {
        Account {
            id: 1,
            name: "test".to_string(),
            kind,
            email: "me@mail.example".to_string(),
            project_tag: None,
            settings,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn non_imap_account_is_skipped_as_not_imap() {
        let a = account(AccountKind::Gmail, serde_json::json!({}));
        match imap_config_from_account(&a) {
            Err(SkipReason::NotImap { kind }) => assert_eq!(kind, "gmail"),
            other => panic!("expected NotImap, got {other:?}"),
        }
    }

    #[test]
    fn imap_account_without_host_is_skipped_as_no_host() {
        let a = account(AccountKind::Imap, serde_json::json!({}));
        match imap_config_from_account(&a) {
            Err(SkipReason::NoHost) => {}
            other => panic!("expected NoHost, got {other:?}"),
        }
    }

    #[test]
    fn imap_account_with_full_settings_builds_config() {
        let a = account(
            AccountKind::Imap,
            serde_json::json!({
                "host": "imap.mail.example",
                "port": 143,
                "username": "someone@mail.example",
                "starttls": true,
            }),
        );
        let config = imap_config_from_account(&a).unwrap();
        assert_eq!(config.account_id, 1);
        assert_eq!(config.host, "imap.mail.example");
        assert_eq!(config.port, 143);
        assert_eq!(config.username, "someone@mail.example");
        assert!(config.starttls);
    }

    #[test]
    fn imap_account_without_username_falls_back_to_email() {
        let a = account(
            AccountKind::Imap,
            serde_json::json!({
                "host": "imap.mail.example",
            }),
        );
        let config = imap_config_from_account(&a).unwrap();
        assert_eq!(config.username, "me@mail.example");
    }

    #[test]
    fn imap_account_without_port_defaults_to_993() {
        let a = account(
            AccountKind::Imap,
            serde_json::json!({
                "host": "imap.mail.example",
            }),
        );
        let config = imap_config_from_account(&a).unwrap();
        assert_eq!(config.port, 993);
    }

    #[test]
    fn sync_all_accounts_succeed_is_ok() {
        let o = SyncOutcome {
            attempted: 2,
            failed: 0,
            skipped: 0,
        };
        assert!(sync_exit_result(&o).is_ok());
    }

    #[test]
    fn sync_some_accounts_fail_is_err() {
        let o = SyncOutcome {
            attempted: 2,
            failed: 1,
            skipped: 0,
        };
        assert!(sync_exit_result(&o).is_err());
    }

    #[test]
    fn sync_all_accounts_fail_is_err() {
        let o = SyncOutcome {
            attempted: 1,
            failed: 1,
            skipped: 0,
        };
        assert!(sync_exit_result(&o).is_err());
    }

    #[test]
    fn sync_no_targets_at_all_is_err() {
        let o = SyncOutcome {
            attempted: 0,
            failed: 0,
            skipped: 0,
        };
        assert!(sync_exit_result(&o).is_err());
    }

    #[test]
    fn sync_no_targets_but_some_skipped_is_err() {
        let o = SyncOutcome {
            attempted: 0,
            failed: 0,
            skipped: 2,
        };
        assert!(sync_exit_result(&o).is_err());
    }
}
