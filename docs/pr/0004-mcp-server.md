# P1: MCP サーバ

## What

Claude Desktop / Claude Code / Cowork から Meowbox のメールを検索・要約・タスク抽出・
下書き作成できるようにする MCP サーバ `meowbox-mcp` を実装した。

- **独立した stdio バイナリ**（`crates/mailmcp` の `[[bin]] name = "meowbox-mcp"`）。
  `mailstore::paths::default_data_dir()` で GUI・CLI と同じ DB を開く。stdout は
  JSON-RPC 専用で、ログはすべて stderr（`tracing_subscriber` の `with_writer`）
- **ツール 11 個**（`rmcp` の `#[tool_router]`）:
  `list_accounts` / `list_projects` / `search_messages` / `get_thread` /
  `get_message` / `get_attachment` / `inbox_digest` / `save_summary` /
  `upsert_tasks` / `list_tasks` / `create_draft`
- **出さないもの** — 送信系と `mark`（既読・アーカイブ）。`list_accounts` /
  `list_projects` は `settings_json`（host / port / username）とパスワードを含めない。
  全ツールの description に日英併記で「送信はできません。既読状態は変更されません。」
  を必ず含める（`SAFETY_NOTE_JA` / `SAFETY_NOTE_EN`）
- **`meta` への `last_synced_at` の永続化** — `Store::set_account_synced_at` /
  `account_synced_at` を追加し、`sync:<account_id>:finished_at` に記録する
  （別プロセスの MCP から GUI のメモリを読めないため）
- **`upsert_tasks` の同一性** — `source_message_id` + `title` で既存行を更新し、
  `status` は上書きしない（`Store::upsert_task`）
- **配布** — Tauri の `bundle.externalBin`（`binaries/meowbox-mcp`）に同梱。
  `apps/desktop/scripts/copy-mcp-sidecar.mjs`（`pnpm mcp:sidecar`）が
  `cargo build --release -p mailmcp`（`pnpm mcp:build`）の成果物をターゲットトリプル
  付きのファイル名にリネームコピーする
- **UI** — 設定モーダルに「Claude 連携」節を追加。`mcp_integration` コマンドが
  `meowbox-mcp` の実行ファイルパスを解決し、`claude_desktop_config.json` 用の JSON と
  `claude mcp add` のコマンドをコピーできる形で返す。パスは自動検出するだけで、
  設定ファイルの書き換えはしない
- **ドキュメント** — [ADR 0007](../adr/0007-mcp-server.md)、README の
  「Claude と繋ぐ」節

## Why

[ADR 0007](../adr/0007-mcp-server.md) を参照。要点だけ再掲する。

- 独立 stdio バイナリにしたのは、GUI が起動していなくても Claude から使えるため。
  ポートや認証トークンの管理も要らず、stdio は 3 クライアントで同じ登録手順が使える
- `mark` を出さないのは、既読・アーカイブが「人間がそのメールを片付けた」という
  事実の記録であり、Claude が読んだことと意味が違うため。ここを共有すると
  人間が未読の山で状況を把握するという使い方が壊れる
- `upsert_tasks` で `status` を上書きしないのは、Claude がスレッドを読み直すたびに
  呼び得るため。片付けたタスクが蒸し返されるのを防ぐ
- 設定ファイルを自動で書き換えないのは、他の MCP サーバの設定も同居する共有ファイルを
  アプリが仮定して壊すリスクを避けるため。コピペにしておけば人間が見ながら直せる

## ツール一覧

| ツール | 役割 |
|---|---|
| `list_accounts` | アカウント一覧（秘密情報を含まない） |
| `list_projects` | 案件（project_tag）ごとのアカウントまとめ |
| `search_messages` | 全文検索。返り値は軽量 |
| `get_thread` | スレッドを時系列で1回取得。`include_quotes` で引用の有無を切り替え |
| `get_message` | 1通を本文・引用・添付一覧つきで取得 |
| `get_attachment` | 添付を取り出しローカルパスを返す |
| `inbox_digest` | 未読メールをスレッド単位でまとめたダイジェスト |
| `save_summary` | Claude が作った要約を保存 |
| `upsert_tasks` | タスク抽出結果を保存（`status` は上書きしない） |
| `list_tasks` | タスク一覧 |
| `create_draft` | 返信下書きを保存（送信はしない） |

送信系と `mark` は無い。

## テストの状況

実行して確認した件数（2026-09-06 時点）:

```
cargo test --workspace 2>&1 | Select-String "test result"
```
→ 全クレート/バイナリ分の結果を合計すると **188 passed, 0 failed, 1 ignored**
（ignored は `#[ignore]` の実 IMAP 接続テスト。`MEOWBOX_TEST_IMAP_*` があるときだけ
手元で走る）。`crates/mailmcp/tests/stdio.rs` に、`meowbox-mcp` を子プロセスとして
起動し JSON-RPC を stdio で往復させる統合テストがあり、`tools/list` が期待の
11 個ちょうどで `mark` や送信系が無いこと、`list_accounts` の返り値に
`settings` / `host` / `port` / `username` / `password` が含まれないこと、
`save_summary` → `get_thread`、`upsert_tasks` の insert/update 切り替えなどを確認している

```
cd apps/desktop; pnpm test
```
→ **110 passed**（18 ファイル、0 failed）

## レビューで直したもの

なし（今回のレビューでの指摘は無かった）。

## 積み残し

| 項目 | 送り先 |
|---|---|
| `get_message` の `include_html` は無視される（`Store` が `body_html` を返さない） | 未定 |
| MCP から GUI への push 通知は無く、フォーカス再読込（既定オフの 60 秒ポーリングあり）で代用 | 未定 |
| 要約・タスク抽出・下書き・承認送信を実データに繋ぐ | P3-b |
| Gmail / Microsoft 365 | P4 |

## Checklist

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`（188 passed / 1 ignored）
- [x] `pnpm lint`
- [x] `pnpm typecheck`
- [x] `pnpm test`（110 passed / 18 ファイル）
- [x] ADR 0007
- [x] `CLAUDE.md` の P1 にチェック
- [x] README に「Claude と繋ぐ」節
