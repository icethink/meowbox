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
crates/mailmcp    MCP サーバ。`meowbox-mcp` バイナリ（rmcp、stdio）。実装済み
crates/mailcli    `meowbox` バイナリ。UI/MCP なしで動くデバッグ入口
apps/desktop      Tauri v2 + React + TypeScript + Tailwind v4（pnpm）。
                  `src-tauri/commands/` に Tauri コマンド、`src-tauri/state.rs` に `AppState`
```
依存方向は **core ← store ← sync ← (mcp, cli, desktop)**。逆流させない。

## コマンド
```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p mailcli -- accounts list
cargo run -p mailmcp --bin meowbox-mcp

# デスクトップ（apps/desktop 配下）
cd apps/desktop && pnpm test
cd apps/desktop && pnpm lint
cd apps/desktop && pnpm typecheck
cd apps/desktop && pnpm build
```
`src-tauri` を含む cargo コマンド（`cargo clippy --workspace` / `cargo test --workspace` など）の前には
`pnpm mcp:build && pnpm mcp:sidecar` が要る。externalBin の実体が無いと build.rs が落ちるため。

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
11. UI に出す表示用の文字列（相対時刻・「今日」の境界）は Rust 側で作らない。
    API は RFC 3339 の日時だけを返し、`src/lib/relativeDate.ts` が組み立てる。
12. **MCP に送信ツールと `mark` を出さない。** Claude が書けるのは要約・タスク・下書きだけ。
    アカウントの接続設定（host / port / username）とパスワードは MCP の返り値に含めない。
13. **MCP のツールを足したり返り値を変えたりしたら、実クライアントから呼んで確かめる。**
    Claude Code か Claude Desktop から実際に呼び出し、schema validation を通すこと。
    stdio テストだけでは MCP の仕様適合を担保できない
    （PR #5: `structuredContent` はオブジェクト必須という制約に stdio テストは気づけず、
    実クライアントから呼んで初めて落ちた）。

## 現在のフェーズと次の一手
P1 まで完了。次は P3-b / P4 / P6。
- [x] P0-a: workspace 雛形、スキーマ、`meowbox init / accounts / search`
- [x] P2: Tauri UI（AppShell / Sidebar / ThreadList / ThreadView / DigestPanel、
      キーボード操作、モックデータ）— 2026-09-03
- [x] P0-b: `mailsync::imap` を async-imap で実装、`parse` を mail-parser で実装、`meowbox sync` を動かす
      （最初のターゲット: 汎用 IMAP 1 アカウント、INBOX の直近 90 日）
      normalize_subject に RE: / Re[2]: / FW: / 返信：（全角）などを含むテストを追加する — 2026-09-04
- [x] P3-a: `apps/desktop/src/api/` のモックを Tauri invoke → mailstore に差し替えた。
      アカウント登録ウィザード（種別 → サーバー設定 + 接続テスト → 案件タグ）、
      同期の進捗イベント（`sync://progress`）、スレッド・一覧の実データ表示、
      アカウントの再同期・削除ができる設定モーダルを実装した — 2026-09-06
- [x] P1: `mailmcp` を rmcp で実装し、`meowbox-mcp` を独立 stdio バイナリとして配布
      （`bundle.externalBin`）。ツールは list_accounts / list_projects /
      search_messages / get_thread / get_message / get_attachment / inbox_digest /
      save_summary / upsert_tasks / list_tasks / create_draft の 11 個。送信系と
      `mark` は出さない。設定モーダルの「Claude 連携」節から登録用の JSON /
      コマンドをコピーできる — 2026-09-06
- [ ] P3-b: 要約・タスク抽出・下書き生成・承認送信を実データに繋ぐ
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
- 並行作業中は `git stash` を使わない。他の作業者の変更まで巻き込んで消す事故があった。
- PR を書くときは `docs/pr/TEMPLATE.md` のチェックリストを使う。

## スタイル
- コメントは日本語でよい。識別子は英語。
- コミットメッセージは英語 1 行 + 必要なら日本語本文。
- PR は小さく。1 PR = 1 フェーズの 1 項目。
