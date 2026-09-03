//! `meowbox` CLI。UI も MCP も無しで同期・検索ができるデバッグ入口。
//!
//!   meowbox init
//!   meowbox accounts add --name work --kind imap --email me@example.com --project 案件A \
//!       --host imap.example.com --port 993
//!   meowbox accounts list
//!   meowbox sync [--account 1]          (P0 で実装)
//!   meowbox search "見積" --project 案件A --unread
//!
//! DB の場所は `--db` か `MEOWBOX_DB`、既定は `./data/meowbox.db`。

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mailcore::AccountKind;
use mailstore::{SearchQuery, Store};

#[derive(Parser)]
#[command(name = "meowbox", version, about = "Meowbox — AI-friendly mail aggregator")]
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
        #[arg(long)]
        account: Option<i64>,
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
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
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
            } => {
                let kind = AccountKind::parse(&kind)
                    .with_context(|| format!("unknown kind '{kind}' (imap|gmail|m365)"))?;
                let settings = serde_json::json!({ "host": host, "port": port });
                let a = store.add_account(&name, kind, &email, project.as_deref(), &settings)?;
                println!("added account #{} {}", a.id, a.email);
                println!("note: パスワード/トークンは keyring に保存する予定（P0 未実装）");
            }
        },
        Cmd::Sync { account } => {
            // TODO(P0): アカウント設定から ImapBackend を組み立てて SyncEngine::sync_once
            anyhow::bail!(
                "sync is not implemented yet (P0). account={account:?} — see crates/mailsync"
            );
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
    }
    Ok(())
}
