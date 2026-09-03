//! IMAP の実接続テスト。
//!
//! CI では走らない。手元で `cargo test -p mailsync -- --ignored` で走らせる。
//!
//! 環境変数 `MEOWBOX_TEST_IMAP_HOST` / `_PORT` / `_USER` / `_PASSWORD` / `_STARTTLS` が
//! すべて揃っているときだけ実行する。1 つでも欠けていれば早期 return する。
//! パスワードはこのテストが keyring（`account:-1`）に書き込んでから
//! `ImapBackend` に読ませる。値そのものはログにも assert メッセージにも出さない。

use mailcore::MailBackend;
use mailsync::imap::{ImapBackend, ImapConfig};

/// このテスト専用の仮想アカウント ID。他のテストや実データと衝突しない値を使う。
const TEST_ACCOUNT_ID: i64 = -1;

fn env_or_skip(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("{name} が未設定のため imap_live テストをスキップします");
            None
        }
    }
}

#[tokio::test]
#[ignore]
async fn list_folders_includes_inbox() {
    let Some(host) = env_or_skip("MEOWBOX_TEST_IMAP_HOST") else {
        return;
    };
    let Some(port) = env_or_skip("MEOWBOX_TEST_IMAP_PORT") else {
        return;
    };
    let Some(username) = env_or_skip("MEOWBOX_TEST_IMAP_USER") else {
        return;
    };
    let Some(password) = env_or_skip("MEOWBOX_TEST_IMAP_PASSWORD") else {
        return;
    };
    let Some(starttls) = env_or_skip("MEOWBOX_TEST_IMAP_STARTTLS") else {
        return;
    };

    let port: u16 = port
        .parse()
        .expect("MEOWBOX_TEST_IMAP_PORT must be a valid port number");
    let starttls = starttls == "1" || starttls.eq_ignore_ascii_case("true");

    // `ImapBackend` はパスワードを keyring からしか読まないので、テスト用アカウント ID に
    // 一度だけ書いておく。
    let entry = keyring::Entry::new("meowbox", &format!("account:{TEST_ACCOUNT_ID}"))
        .expect("failed to open keyring entry for the test account");
    entry
        .set_password(&password)
        .expect("failed to store the test password in the keyring");

    let backend = ImapBackend::new(ImapConfig {
        account_id: TEST_ACCOUNT_ID,
        host,
        port,
        username,
        starttls,
    });

    let folders = backend
        .list_folders()
        .await
        .expect("list_folders should succeed against the configured server");

    assert!(
        folders
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("INBOX")),
        "folder list should include INBOX"
    );
}
