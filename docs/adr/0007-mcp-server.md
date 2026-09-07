# 7. MCP サーバの作り方（P1）

- 状態: 採用
- 日付: 2026-09-06

## 背景

P1 の目的は、Claude Desktop / Claude Code / Cowork から Meowbox のメールを
検索・要約・タスク抽出・下書き作成できるようにすること。これが動けば、これまで
使っていた Thunderbird MCP を卒業できる。

形態（アプリ内 HTTP か独立バイナリか）と、DB を複数プロセス（GUI・同期タスク・MCP）で
共有する作法を先に決める必要がある。

## 決定

1. **独立した stdio バイナリ**（`meowbox-mcp`、`crates/mailmcp` の `[[bin]]`）。
   Claude 側がプロセスを起動して stdio で話す。**アプリ内 HTTP（案 B）は採らない**。

2. **DB の共有** — `mailstore::paths::default_data_dir()` で GUI・CLI と同じ DB を開く。
   `Store::open` / `Store::open_in_memory` の両方で `PRAGMA busy_timeout = 5000` を
   設定してある。WAL なので読みは並行して問題ない。

3. **stdout は JSON-RPC 専用** — ログは stderr にだけ出す（`tracing_subscriber` の
   `with_writer(std::io::stderr)`）。`println!` を使わない（stdout に 1 行でも混ざると
   プロトコルが壊れる）。

4. **出さないツール** — 送信系（ADR 0002 の継続）と **`mark`（既読・アーカイブ）**。
   `mark` を出すと、人間が見ていないメールを Claude が既読にしてしまう事故が起きうる。
   状態を変えるのは人間の操作だけにする。

5. **返さない情報** — `settings_json`（host / port / username）とパスワード。
   `list_accounts` / `list_projects` が返すのは id / name / email / project_tag /
   kind / last_synced_at / unread_count（`list_projects` はさらに軽量な
   id / email だけ）に絞る。

6. **`last_synced_at` を `meta` に永続化** — 別プロセスの MCP からは GUI のメモリを
   読めないため、`Store::set_account_synced_at` が同期成功時に
   `sync:<account_id>:finished_at` を `meta` に書く。キーの形式（`synced_at_key`）は
   `mailstore` と `apps/desktop/src-tauri/src/commands/sync.rs` の両方にテストがあり、
   固定してある。

7. **`upsert_tasks` の同一性** — `source_message_id` + `title` で既存行を探して更新する。
   **`status` は上書きしない**（`Store::upsert_task` が `UPDATE` の対象列から `status` を
   外している）。人間が done / dismissed にしたものを Claude が open に戻さないため。

8. **「今日」を知らない** — `inbox_digest` の `since` は Claude が RFC3339 で渡す。
   省略時は `Utc::now() - 24時間`（サーバはタイムゾーンも「今日」の境界も知らない）。

9. **配布** — Tauri の `bundle.externalBin`（`binaries/meowbox-mcp`）でインストーラに
   同梱する。Tauri はターゲットトリプル付きのファイル名（`meowbox-mcp-<triple>(.exe)`）を
   要求するので、`apps/desktop/scripts/copy-mcp-sidecar.mjs` を
   `cargo build --release -p mailmcp`（`pnpm mcp:build`）の後・`tauri build` の前に
   噛ませ、`rustc -vV` の `host:` からトリプルを取って `src-tauri/binaries/` へ
   リネームコピーする（`pnpm mcp:sidecar`）。

10. **登録は手貼り** — 設定モーダルの「Claude 連携」節に
    `claude_desktop_config.json` 用の JSON（`mcp_integration` コマンドが組み立てる）と
    `claude mcp add` のコマンドを出し、コピーできるようにする。
    **アプリが設定ファイルを自動で書き換えることはしない**（他人の設定を壊さない）。

11. **UI への反映** — ウィンドウが `focus` イベントを受けたら読み直す
    （`useRefreshOnFocus`、連打対策で 2 秒スロットル）。60 秒ポーリング
    （`useAutoRefresh`）は用意してあるが既定オフ（`autoRefresh: false`）。

## 理由

- **なぜ独立バイナリか** — GUI（Tauri アプリ）が起動していなくても Claude Desktop /
  Claude Code / Cowork から使える。ポートや認証トークンの管理が要らない。
  stdio は MCP の一番普通の形なので、3 つのクライアントで同じ登録手順が使える。
- **なぜアプリ内 HTTP をやらないか** — アプリの起動が前提になってしまい、
  「メールを見ていないときは Claude からも使えない」状態になる。ポートの空き確認と
  トークンの発行・保管という、stdio では要らない管理が増えるだけで得るものが無い。
- **なぜ `mark` を出さないか** — 既読・アーカイブは「人間がそのメールを見た（片付けた）」
  という事実の記録であり、Claude が検索や要約のためにメールを読んだことと意味が違う。
  ここを共有すると、Claude が触れただけで「読んだこと」になってしまい、
  人間が後から未読の山を見て状況を把握するという使い方が壊れる。
- **なぜ設定ファイルを自動で書き換えないか** — `claude_desktop_config.json` は
  Meowbox 以外の MCP サーバの設定も同居する共有ファイル。アプリが構造を仮定して
  書き換えると、他のツールの設定を壊す・フォーマットを破壊するリスクがある。
  コピペにしておけば、失敗するとしても人間が見ながら直せる。
- **`status` を上書きしない理由** — Claude はスレッドを読み直すたびに `upsert_tasks` を
  呼び得る。そのたびに `status` を `open` に戻すと、片付けたはずのタスクが
  蒸し返される。抽出結果の更新は due / confidence / created_by に留める。

## 結果

- 実装したツール（11 個）: `list_accounts` / `list_projects` / `search_messages` /
  `get_thread` / `get_message` / `get_attachment` / `inbox_digest` / `save_summary` /
  `upsert_tasks` / `list_tasks` / `create_draft`。送信系と `mark` は無い。
- 全ツールの description に日英併記で「送信はできません。既読状態は変更されません。」
  （`SAFETY_NOTE_JA` / `SAFETY_NOTE_EN`）を必ず含める。
- テスト — `crates/mailmcp/tests/stdio.rs` に、実バイナリを子プロセスとして起動して
  JSON-RPC を stdio で往復させる統合テストがある。`tools/list` が期待の 11 個
  ちょうどであること（`mark` や送信系が無いこと）、`list_accounts` の返り値に
  `settings` / `host` / `port` / `username` / `password` の文字列が含まれないこと、
  `save_summary` → `get_thread`、`upsert_tasks` の insert/update 切り替えなど、
  ツールの往復を一通り確認している。`*_impl` 関数の単体テストは別に `main.rs` にある。
- 積み残し（コードの TODO を確認したもの）:
  - `get_message` の `include_html` は `Store` がまだ `body_html` を返さないため
    無視している（`crates/mailmcp/src/lib.rs` / `main.rs` の TODO コメント）
  - MCP から GUI への push 通知は無い。UI はフォーカス再読込
    （`useRefreshOnFocus`）と、既定オフの 60 秒ポーリング（`useAutoRefresh`）で
    代用している

## 追記 (2026-09-07)

MCP の `structuredContent` はオブジェクトでなければならず、トップレベルで配列を
直接返すとクライアントのスキーマ検証に落ちる。そのため `list_accounts` /
`list_projects` / `list_tasks` / `search_messages` は、それぞれ
`{ accounts: [...] }` / `{ projects: [...] }` / `{ tasks: [...] }` /
`{ messages: [...], truncated }` という形でオブジェクトに包んで返す。

## 追記 (2026-09-07) — 使い勝手の調整

決定 8（「今日」を知らない）の延長で、「Claude に往復させない」という方針をさらに
何点か引数に反映した。

- `search_messages` に `include_body`（既定 false）。true のとき各結果に
  `body_text` の先頭 2,000 文字を付ける。全文検索の結果を見てから
  もう一度 `get_message` を呼ぶ往復を減らすため。
- `inbox_digest` に `limit`（既定 20、最大 50）。案件を `project` で絞った上で
  件数も調整できるようにした。
- `create_draft` に `reply_all`（既定 false）。true のとき、元メールの
  差出人 + to + cc から自分のアドレスを除いたものを宛先にする。`to` を
  明示したときはそちらを優先する。全員返信のたびに Claude が宛先一覧を
  組み立て直す必要をなくす。
- `get_message` の `include_html` を実装した（積み残しだった項目）。true のとき
  `body_html` を返す（元メールに HTML パートが無ければ null のまま）。
- `upsert_tasks` の `account_id` を省略可にした。省略時は `source_message_id`
  から対応するメールのアカウントを推定する。`account_id` と `source_message_id`
  の両方が無いタスクはエラーにする。

これに合わせて、エラーメッセージを日英併記（`{ja} / {en}`）に統一した。
Claude Desktop / Claude Code は英語で動くこともあるため、日本語だけのエラーは
文脈が伝わらないことがあったための対応。

全ツールの description の末尾に必ず付けている安全文言（送信不可・既読状態不変）は、
`SAFETY_NOTE_JA` / `SAFETY_NOTE_EN`（`mailmcp::lib`）と `safety_note_ja!` /
`safety_note_en!`（`main.rs` のマクロ）から組み立てるようにし、両者の文字列が
一致することをテストで固定した。実装上の注意として、`rmcp-macros` の
`#[tool(description = ...)]` は文字列リテラルしか受け付けないため、マクロ展開の
`concat!(...)` を渡すには `#[tool]` を素で付けたうえで `#[doc = concat!(...)]` を
使っている。

送信ツールと `mark` を出さない方針（決定 4）は変わっていない。
