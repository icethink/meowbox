//! `meowbox-mcp` を子プロセスとして起動し、stdio 越しに JSON-RPC を往復させる統合テスト。
//!
//! `*_impl` の単体テストだけでは「ツール名・引数名・返り値の形が実際に噛み合っているか」
//! （＝ rmcp の JSON-RPC トランスポートの形と一致しているか）が分からないため、
//! 実バイナリを起動して確かめる。
//!
//! **重要**: このテストは必ず `MEOWBOX_DATA_DIR` を一時ディレクトリに向けて子プロセスを
//! 起動すること。向け忘れると開発機の本物の DB
//! （既定では `%APPDATA%\dev.icethink.meowbox\meowbox.db` など）を触ってしまう。

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use mailcore::{AccountKind, Address};
use mailstore::{NewMessage, Store};
use serde_json::{json, Value};

/// 1 回の応答待ちに許す最大時間。これを超えたら「ハングした」とみなして panic する
/// （CI を無限に止めないため）。
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// `crates/mailsync/tests/fixtures/<name>` を読む。
/// `env!("CARGO_MANIFEST_DIR")` を起点にするので、CI の作業ディレクトリに依存しない。
fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mailsync/tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

/// フィクスチャ 1 通を raw .eml として書き出し、パースして DB に入れる。
/// `thread_key` は呼び出し側が明示的に決める（複数メッセージを同じスレッドにまとめるため）。
#[allow(clippy::too_many_arguments)]
fn insert_fixture_message(
    store: &Store,
    mail_root: &Path,
    account_id: i64,
    folder_id: i64,
    uid: u32,
    fixture_name: &str,
    thread_key: &str,
    is_read: bool,
) -> (i64, mailsync::parse::Parsed) {
    let raw = fixture_bytes(fixture_name);
    let parsed = mailsync::parse::parse(&raw)
        .unwrap_or_else(|e| panic!("failed to parse fixture {fixture_name}: {e}"));

    let dir = mail_root.join(account_id.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let raw_path = dir.join(format!("{uid}.eml"));
    std::fs::write(&raw_path, &raw).unwrap();
    let raw_path_str = raw_path.to_string_lossy().to_string();

    let default_from = Address {
        name: None,
        email: String::new(),
    };
    let from = parsed.from.as_ref().unwrap_or(&default_from);
    let snippet = mailsync::parse::snippet(&parsed.body_text, 120);

    let new_message = NewMessage {
        account_id,
        folder_id,
        uid,
        message_id: parsed.message_id.as_deref(),
        thread_key,
        from,
        to: &parsed.to,
        cc: &parsed.cc,
        subject: &parsed.subject,
        // フィクスチャの Date ヘッダは過去の固定日時なので、`inbox_digest`
        // （直近 24 時間）に引っかかるよう挿入時刻で上書きする。
        date: Utc::now(),
        snippet: &snippet,
        body_text: &parsed.body_text,
        body_html: parsed.body_html.as_deref(),
        has_attachments: parsed.has_attachments,
        is_read,
        is_flagged: false,
        raw_path: Some(&raw_path_str),
    };
    let id = store
        .insert_message(&new_message)
        .unwrap()
        .unwrap_or_else(|| {
            panic!("insert_message returned None for {fixture_name}（uid が重複している）")
        });
    (id, parsed)
}

/// `meowbox-mcp` を起動し、JSON-RPC を stdio 越しにやり取りするヘルパ。
struct Server {
    child: Child,
    stdin: ChildStdin,
    /// 子プロセスの stdout を 1 行ずつ読む別スレッドからのチャンネル。
    /// 読み取りをメインスレッドから切り離すことで、応答が来ないときに
    /// テストがハングしないようにする（`recv_timeout` で必ず抜ける）。
    lines_rx: Receiver<String>,
    stderr: Arc<Mutex<Vec<String>>>,
    next_id: i64,
}

impl Server {
    fn start(data_dir: &Path) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_meowbox-mcp"))
            .env("MEOWBOX_DATA_DIR", data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start meowbox-mcp");

        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        {
            let stderr_lines = Arc::clone(&stderr_lines);
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    stderr_lines.lock().unwrap().push(line);
                }
            });
        }

        Server {
            child,
            stdin,
            lines_rx: rx,
            stderr: stderr_lines,
            next_id: 0,
        }
    }

    fn stderr_dump(&self) -> String {
        self.stderr.lock().unwrap().join("\n")
    }

    fn recv_line(&self) -> Value {
        let line = self
            .lines_rx
            .recv_timeout(READ_TIMEOUT)
            .unwrap_or_else(|_| {
                panic!(
                "meowbox-mcp から応答がありません（{READ_TIMEOUT:?} でタイムアウト）。stderr:\n{}",
                self.stderr_dump()
            )
            });
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("応答が JSON になっていません: {e}\nline: {line}"))
    }

    fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{msg}").expect("write request");
        self.stdin.flush().expect("flush request");

        loop {
            let resp = self.recv_line();
            if resp.get("id").and_then(Value::as_i64) == Some(id) {
                return resp;
            }
            // id の付かない行（通知など）は無視して次を待つ。
        }
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{msg}").expect("write notification");
        self.stdin.flush().expect("flush notification");
    }

    fn initialize(&mut self) {
        let resp = self.send_request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "meowbox-mcp-stdio-test", "version": "0.0.1" },
            }),
        );
        assert!(
            resp.get("result").is_some(),
            "initialize がエラーになりました: {resp:?}\nstderr:\n{}",
            self.stderr_dump()
        );
        self.send_notification("notifications/initialized", json!({}));
    }

    fn list_tools(&mut self) -> Vec<Value> {
        let resp = self.send_request("tools/list", json!({}));
        resp["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| {
                panic!(
                    "tools/list の結果が不正です: {resp:?}\nstderr:\n{}",
                    self.stderr_dump()
                )
            })
            .clone()
    }

    /// `tools/call` の生の結果（CallToolResult 相当の JSON）を返す。
    /// エラーかどうかは呼び出し側が `isError` を見て判定する（ここではパニックしない）。
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let resp = self.send_request("tools/call", json!({ "name": tool, "arguments": args }));
        resp.get("result").cloned().unwrap_or_else(|| {
            panic!(
                "{tool} の呼び出しが JSON-RPC レベルのエラーになりました: {resp:?}\nstderr:\n{}",
                self.stderr_dump()
            )
        })
    }

    /// 成功が期待される呼び出し。`structuredContent`（= ツールの戻り値そのもの）を返す。
    fn call_ok(&mut self, tool: &str, args: Value) -> Value {
        let result = self.call(tool, args);
        assert_ne!(
            result.get("isError").and_then(Value::as_bool),
            Some(true),
            "{tool} がエラーを返しました: {result:?}\nstderr:\n{}",
            self.stderr_dump()
        );
        result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| panic!("{tool} の structuredContent がありません: {result:?}"))
    }

    /// エラーが期待される呼び出し。
    fn call_err(&mut self, tool: &str, args: Value) -> Value {
        let result = self.call(tool, args);
        assert_eq!(
            result.get("isError").and_then(Value::as_bool),
            Some(true),
            "{tool} はエラーになるはずでした: {result:?}"
        );
        result
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn exercises_every_tool_over_stdio() {
    // **重要**: MEOWBOX_DATA_DIR は必ず一時ディレクトリに向ける。
    // 向け忘れると開発機の本物のメールデータベースを読み書きしてしまう。
    let data_dir = tempfile::tempdir().unwrap();
    let db_path = mailstore::paths::db_path(data_dir.path());
    let mail_root = mailstore::paths::mail_dir(data_dir.path());
    std::fs::create_dir_all(&mail_root).unwrap();

    const THREAD_1: &str = "thread-1"; // 既読の元メール + 未読の返信（引用あり）
    const THREAD_2: &str = "thread-2"; // 添付付き、未読
    const THREAD_3: &str = "thread-3"; // 案件タグ無しアカウント、未読

    let (account_a, msg_c_id, attachment_id) = {
        let store = Store::open(&db_path).unwrap();

        let account_a = store
            .add_account(
                "メール A",
                AccountKind::Imap,
                "account-a@mail-a.example",
                Some("project-a"),
                &json!({}),
            )
            .unwrap()
            .id;
        let account_b = store
            .add_account(
                "メール B",
                AccountKind::Imap,
                "account-b@mail-b.example",
                None,
                &json!({}),
            )
            .unwrap()
            .id;

        let folder_a = store.ensure_folder(account_a, "INBOX", "inbox").unwrap();
        let folder_b = store.ensure_folder(account_b, "INBOX", "inbox").unwrap();

        // thread-1: 既読の元メール（引用なし）+ 未読の返信（引用あり）。
        insert_fixture_message(
            &store,
            &mail_root,
            account_a,
            folder_a,
            1,
            "utf8-alternative.eml",
            THREAD_1,
            true,
        );
        insert_fixture_message(
            &store,
            &mail_root,
            account_a,
            folder_a,
            2,
            "reply-multiprefix.eml",
            THREAD_1,
            false,
        );

        // thread-2: 添付付き、未読。
        let (msg_c_id, parsed_c) = insert_fixture_message(
            &store,
            &mail_root,
            account_a,
            folder_a,
            3,
            "attachment-mixed.eml",
            THREAD_2,
            false,
        );
        let mut attachment_id = 0i64;
        for att in &parsed_c.attachments {
            attachment_id = store
                .insert_attachment_meta(msg_c_id, &att.filename, &att.mime, att.size)
                .unwrap();
        }
        assert!(
            attachment_id > 0,
            "attachment-mixed.eml に添付が見つかりません"
        );

        // thread-3: project_tag の無いアカウント、未読。
        insert_fixture_message(
            &store,
            &mail_root,
            account_b,
            folder_b,
            1,
            "shiftjis-plain.eml",
            THREAD_3,
            false,
        );

        (account_a, msg_c_id, attachment_id)
    };

    let mut server = Server::start(data_dir.path());
    server.initialize();

    // --- tools/list: 11 個ちょうど。mark と送信系は無い。全 description に「送信」を含む。
    let tools = server.list_tools();
    let names: std::collections::BTreeSet<String> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect();
    let expected: std::collections::BTreeSet<String> = [
        "list_accounts",
        "list_projects",
        "search_messages",
        "get_thread",
        "get_message",
        "get_attachment",
        "inbox_digest",
        "save_summary",
        "upsert_tasks",
        "list_tasks",
        "create_draft",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(
        names, expected,
        "tools/list の内容が想定と違います: {tools:?}"
    );
    assert!(!names.contains("mark"));
    assert!(!names.iter().any(|n| n.contains("send")));

    for t in &tools {
        let desc = t["description"].as_str().unwrap_or_default();
        assert!(
            desc.contains("送信"),
            "{} の description に「送信」が含まれていません: {desc}",
            t["name"]
        );
    }

    // --- list_accounts: 2 件、秘密情報を含まない。
    let accounts = server.call_ok("list_accounts", json!({}));
    let accounts_str = accounts.to_string();
    for forbidden in ["settings", "host", "port", "username", "password"] {
        assert!(
            !accounts_str.contains(forbidden),
            "list_accounts の返り値に {forbidden} が含まれています: {accounts_str}"
        );
    }
    assert_eq!(accounts.as_array().unwrap().len(), 2);

    // --- search_messages（query 無し）が結果を返す。
    let search_results = server.call_ok("search_messages", json!({}));
    assert!(
        !search_results.as_array().unwrap().is_empty(),
        "search_messages が空でした: {search_results:?}"
    );

    // --- get_thread: include_quotes による quoted_text の有無。
    let thread1_no_quotes = server.call_ok(
        "get_thread",
        json!({ "thread_key": THREAD_1, "include_quotes": false }),
    );
    let messages = thread1_no_quotes["messages"].as_array().unwrap();
    assert!(
        messages.iter().all(|m| m.get("quoted_text").is_none()),
        "include_quotes=false なのに quoted_text が出ています: {messages:?}"
    );

    let thread1_with_quotes = server.call_ok(
        "get_thread",
        json!({ "thread_key": THREAD_1, "include_quotes": true }),
    );
    let messages = thread1_with_quotes["messages"].as_array().unwrap();
    assert!(
        messages.iter().any(|m| m
            .get("quoted_text")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())),
        "include_quotes=true なのに引用が見つかりません: {messages:?}"
    );

    // --- save_summary → get_thread の往復。
    let save_res = server.call_ok(
        "save_summary",
        json!({
            "target": format!("thread:{THREAD_1}"),
            "model": "claude-test",
            "summary": "要約テスト本文",
        }),
    );
    assert!(save_res["id"].as_i64().unwrap_or(0) > 0);

    let thread1_after_summary = server.call_ok(
        "get_thread",
        json!({ "thread_key": THREAD_1, "include_quotes": false }),
    );
    let summary = &thread1_after_summary["summary"];
    assert_eq!(summary["summary"].as_str(), Some("要約テスト本文"));
    assert_eq!(summary["model"].as_str(), Some("claude-test"));

    // --- upsert_tasks を 2 回。1 回目 inserted、2 回目 updated。list_tasks は 1 件だけ。
    let task_args = json!({
        "tasks": [{
            "account_id": account_a,
            "source_message_id": msg_c_id,
            "title": "見積の確認",
            "due": null,
            "confidence": 0.9,
        }]
    });
    let first = server.call_ok("upsert_tasks", task_args.clone());
    assert_eq!(first["inserted"].as_i64(), Some(1));
    assert_eq!(first["updated"].as_i64(), Some(0));

    let second = server.call_ok("upsert_tasks", task_args);
    assert_eq!(second["inserted"].as_i64(), Some(0));
    assert_eq!(second["updated"].as_i64(), Some(1));

    let tasks = server.call_ok("list_tasks", json!({}));
    assert_eq!(tasks.as_array().unwrap().len(), 1);

    // --- create_draft: {id} を返し、送信されない（DB 上は status = "draft"）。
    let draft_res = server.call_ok(
        "create_draft",
        json!({
            "account_id": account_a,
            "in_reply_to": msg_c_id,
            "body": "承知しました。",
        }),
    );
    let draft_id = draft_res["id"].as_i64().expect("draft id");
    assert!(draft_id > 0);
    {
        let store = Store::open(&db_path).unwrap();
        let draft = store.get_draft(draft_id).unwrap().expect("draft exists");
        assert_eq!(draft.status, "draft");
    }

    // --- inbox_digest が groups を返す。
    let digest = server.call_ok("inbox_digest", json!({}));
    assert!(
        !digest["groups"].as_array().unwrap().is_empty(),
        "inbox_digest の groups が空でした: {digest:?}"
    );

    // --- get_attachment: 実在するパスを返し、一時ディレクトリの中にあること。
    let attachment_out = server.call_ok("get_attachment", json!({ "id": attachment_id }));
    let path = attachment_out["path"].as_str().expect("attachment path");
    assert!(
        Path::new(path).exists(),
        "添付ファイルが存在しません: {path}"
    );
    let canon_path = Path::new(path).canonicalize().unwrap();
    let canon_root = data_dir.path().canonicalize().unwrap();
    assert!(
        canon_path.starts_with(&canon_root),
        "添付ファイルが一時ディレクトリの外にあります: {path}"
    );

    // --- 不正な引数（model が空）はエラー応答になる（パニックしない）。
    server.call_err(
        "save_summary",
        json!({
            "target": format!("thread:{THREAD_1}"),
            "model": "",
            "summary": "要約",
        }),
    );
}
