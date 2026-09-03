# CLAUDE.md — Meowbox

Meowbox は Rust 製の AI フレンドリーなメーラー。案件ごとに配布される複数のメールアドレスを
1 つに束ね、Claude が MCP 経由で検索・要約・タスク抽出・返信下書きを行えるようにする。
全体設計は `docs/DESIGN.md`。ここには「作業するときの約束」だけ書く。

## 最初に読むもの
- `docs/DESIGN.md` — アーキテクチャ・スキーマ・MCP ツール一覧・ロードマップ
- `crates/mailcore/src/lib.rs` — ドメイン型と `MailBackend` トレイト
- `crates/mailstore/src/schema.sql` — DB スキーマ（正）
- `docs/design/` — UI デザイン（`main-dark.png` が見本、`tokens.css` が色/字/余白の正、`main-dark.reference.html` が寸法参照）

## ワークスペース
```
crates/mailcore   ドメイン型・トレイト。I/O 禁止・UI 禁止・DB 禁止
crates/mailstore  SQLite（rusqlite bundled, FTS5 trigram）
crates/mailsync   IMAP/Gmail/Graph バックエンド + MIME パース + 同期エンジン
crates/mailmcp    MCP サーバ（rmcp 予定）
crates/mailcli    `meowbox` バイナリ。UI/MCP なしで動くデバッグ入口
apps/desktop      Tauri v2 + React + TypeScript（P2 で作成）
```
依存方向は **core ← store ← sync ← (mcp, cli, desktop)**。逆流させない。

## コマンド
```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p mailcli -- accounts list
```
テストは `cargo test` が全部通る状態を維持する。新しい機能は必ず最低 1 本テストを付ける。

## 守ること
1. **送信は MCP に出さない。** `create_draft` まで。送信は UI の承認ボタンだけ（MVP の安全境界）。
2. **秘密情報を DB / ログ / settings_json に書かない。** パスワード・トークンは `keyring`。
3. **MCP ツールは AI が使いやすい粒度で。** 1 通ずつ取らせない。スレッド・ダイジェスト単位で返す。
   返り値は軽量（`MessageSummary`）。本文は `get_thread` / `get_message` のときだけ。
4. **raw .eml は必ずファイルにも保存する**（`data/mail/<account>/<folder>/<uid>.eml`）。
   MCP が落ちていても Claude がファイルとして読める保険。
5. **スキーマ変更は `schema.sql` + `SCHEMA_VERSION` + マイグレーション** の 3 点セット。
6. 日本語メールが前提。ISO-2022-JP / Shift_JIS のデコード、trigram FTS を壊さない。
7. `unwrap()` は test 以外で使わない。エラーは `anyhow` (アプリ層) / `thiserror` (ライブラリ層)。
8. UI は `docs/design/tokens.css` のトークンだけで色を指定する。人間側=`--accent`、Claude 由来=`--ai` を混ぜない。

## 現在のフェーズと次の一手
- [x] P0-a: workspace 雛形、スキーマ、`meowbox init / accounts / search`
- [ ] P0-b: `mailsync::imap` を async-imap で実装、`parse` を mail-parser で実装、`meowbox sync` を動かす
      （最初のターゲット: 汎用 IMAP 1 アカウント、INBOX の直近 90 日）
- [ ] P1: `mailmcp` を rmcp で実装（list_accounts / search_messages / get_thread / inbox_digest）
      → Claude Desktop / Cowork から叩けることを確認したら Thunderbird MCP を卒業
- [ ] P2: Tauri UI
- [ ] P3: 要約・タスク抽出・下書き・承認送信
- [ ] P4: Gmail / M365 OAuth、IDLE、バックフィル

## 追加予定の主要クレート
async-imap, tokio-rustls (or async-native-tls), mail-parser, mail-builder, lettre,
oauth2, keyring, rmcp, reqwest。追加時は workspace.dependencies に集約する。

## スタイル
- コメントは日本語でよい。識別子は英語。
- コミットメッセージは英語 1 行 + 必要なら日本語本文。
- PR は小さく。1 PR = 1 フェーズの 1 項目。
