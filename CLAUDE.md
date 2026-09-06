# CLAUDE.md — Meowbox

Meowbox は Rust 製の AI フレンドリーなメーラー。案件ごとに配布される複数のメールアドレスを
1 つに束ね、Claude が MCP 経由で検索・要約・タスク抽出・返信下書きを行えるようにする。
全体設計は `docs/DESIGN.md`。ここには「作業するときの約束」だけ書く。

## 最初に読むもの
- `docs/DESIGN.md` — アーキテクチャ・スキーマ・MCP ツール一覧・ロードマップ
- `crates/mailcore/src/lib.rs` — ドメイン型と `MailBackend` トレイト
- `crates/mailstore/src/schema.sql` — DB スキーマ（正）
- `docs/design/` — UI デザイン（`main-dark.png` が見本、`tokens.css` が色/字/余白の正、`impl-main-dark.png` が実装の現状）
- `docs/adr/` — 設計判断の記録。方針を変えるときは ADR を足してから実装する
- `apps/desktop/src/api/` — UI から見た唯一のデータ入口。P3 でここだけ `invoke()` に差し替える

## ワークスペース
```
crates/mailcore   ドメイン型・トレイト。I/O 禁止・UI 禁止・DB 禁止
crates/mailstore  SQLite（rusqlite bundled, FTS5 trigram）
crates/mailsync   IMAP/Gmail/Graph バックエンド + MIME パース + 同期エンジン
crates/mailmcp    MCP サーバ（rmcp 予定）
crates/mailcli    `meowbox` バイナリ。UI/MCP なしで動くデバッグ入口
apps/desktop      Tauri v2 + React + TypeScript + Tailwind v4（pnpm）
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
4. **raw .eml は必ずファイルにも保存する**（`<data_dir>/mail/<account_id>/<folder>/<uid>.eml`）。
   `data_dir` の既定は OS のアプリデータディレクトリ配下（Windows なら `%APPDATA%\dev.icethink.meowbox\`）。
   `MEOWBOX_DATA_DIR` で上書きできる。
   MCP が落ちていても Claude がファイルとして読める保険。
5. **スキーマ変更は `schema.sql` + `SCHEMA_VERSION` + マイグレーション** の 3 点セット。
6. 日本語メールが前提。ISO-2022-JP / Shift_JIS のデコード、trigram FTS を壊さない。
7. `unwrap()` は test 以外で使わない。エラーは `anyhow` (アプリ層) / `thiserror` (ライブラリ層)。
8. UI は `docs/design/tokens.css` のトークンだけで色を指定する。人間側=`--accent`、Claude 由来=`--ai` を混ぜない。
   HEX / `rgba()` の直書きは eslint が落とす。足りない値は `tokens.css` に名前を付けてから使い、
   `apps/desktop/src/styles/tokens.css` にも同じ内容を反映する（片方だけ直さない）。
9. Tailwind の `@theme` にキーを足すときは、`--color-*` と `--text-*`（文字サイズ）で
   同じ名前を使わない。両方あると `text-<name>` が色として解決される。
   文字サイズを足したら `--text-<name>--line-height` も必ず一緒に指定する。
10. UI から DB / IPC を直接叩かない。データ取得は `apps/desktop/src/api/` の関数だけを通す。

## 現在のフェーズと次の一手
P0-b まで完了。次は P3。P1 の MCP は実データが入ってから着手する。
- [x] P0-a: workspace 雛形、スキーマ、`meowbox init / accounts / search`
- [x] P2: Tauri UI（AppShell / Sidebar / ThreadList / ThreadView / DigestPanel、
      キーボード操作、モックデータ）— 2026-09-03
- [x] P0-b: `mailsync::imap` を async-imap で実装、`parse` を mail-parser で実装、`meowbox sync` を動かす
      （最初のターゲット: 汎用 IMAP 1 アカウント、INBOX の直近 90 日）
      normalize_subject に RE: / Re[2]: / FW: / 返信：（全角）などを含むテストを追加する — 2026-09-04
- [ ] P3: `apps/desktop/src/api/` のモックを Tauri invoke → mailstore に差し替える。
      `listAccounts / listProjects / listThreads / getThread / getDigest / createDraft` を
      `#[tauri::command]` として `src-tauri` に実装し、UI 側は `src/api/` の中身だけを
      `invoke()` に置き換える（コンポーネントには触らない）。
      あわせて要約・タスク抽出・下書き生成・承認送信を実データに繋ぐ
- [ ] P1: `mailmcp` を rmcp で実装（list_accounts / search_messages / get_thread / inbox_digest）
      → Claude Desktop / Cowork から叩けることを確認したら Thunderbird MCP を卒業
- [ ] P4: Gmail / M365 OAuth、IDLE、バックフィル

## 追加予定の主要クレート
async-imap, tokio-rustls (or async-native-tls), mail-parser, mail-builder, lettre,
oauth2, keyring, rmcp, reqwest。追加時は workspace.dependencies に集約する。

## 作業の割り振り
- メインは計画・判断・レビュー・報告だけを担当し、実作業は `.claude/agents/implementer.md` に 1 単位ずつ委譲する（メインは自分でファイルを編集しない。例外は 1〜2 行の修正）。
- タスクは「仕様が決まっていて判断不要」な単位に分解する。設計が絡む単位（スキーマ変更・依存追加・公開範囲）はメインが方針を書いてから渡す。
- implementer の報告を読んだら `.claude/agents/reviewer.md` に差分を見せ、指摘があれば implementer に差し戻す。「判断が必要」と返ってきたときだけメインが考える。
- 3 回差し戻しても直らない単位は、その 1 回だけ `model: opus` を指定して implementer を呼び直し、どこで上位モデルを使ったかを result.md に書く。
- サブエージェントへの指示は自己完結させる（会話の文脈は渡らない）。対象ファイル・期待する結果・検証コマンドを必ず含める。
- 大きいログは全文を会話に貼らず、`Select-String` / `tail` で必要な行だけ読む。

## スタイル
- コメントは日本語でよい。識別子は英語。
- コミットメッセージは英語 1 行 + 必要なら日本語本文。
- PR は小さく。1 PR = 1 フェーズの 1 項目。
