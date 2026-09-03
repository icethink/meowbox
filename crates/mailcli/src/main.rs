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

            for a in targets {
                if a.kind != AccountKind::Imap {
                    eprintln!(
                        "account #{}: kind '{}' not supported yet",
                        a.id,
                        a.kind.as_str()
                    );
                    continue;
                }
                let host = a.settings.get("host").and_then(|v| v.as_str());
                let Some(host) = host else {
                    eprintln!("account #{}: has no host configured", a.id);
                    continue;
                };
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

                let backend = ImapBackend::new(ImapConfig {
                    account_id: a.id,
                    host: host.to_string(),
                    port,
                    username: username.to_string(),
                    starttls,
                });
                let opts = SyncOptions {
                    only_folder: folder.clone(),
                    since: Some(since),
                    data_dir: PathBuf::from("data/mail"),
                };

                match engine.sync_once(a.id, &backend, &opts).await {
                    Ok(report) => println!(
                        "account #{}: fetched={} inserted={} skipped={} errors={}",
                        a.id, report.fetched, report.inserted, report.skipped, report.errors
                    ),
                    Err(e) => eprintln!("account #{}: sync failed: {e}", a.id),
                }
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
