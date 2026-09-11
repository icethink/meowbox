# P1 続き: MCP の後始末と使い勝手の調整

## What

PR #4（[0004-mcp-server.md](0004-mcp-server.md)）のレビューで積み残しにした 6 件と、
`meowbox-mcp` の引数まわりの使い勝手の調整、それとプロセス面の教訓の反映をまとめた PR。

**PR #4 の積み残し 6 件**
- `crates/mailstore`: `Store::transaction` を追加（all-or-nothing の書き込み）。
  `upsert_tasks_impl` は先に全タスクを検証してから、書き込みは 1 トランザクションに
  まとめるように変更（`due` が不正なタスクが 1 件でもあると、それより前のタスクも
  一切書き込まれない）
- `crates/mailmcp`: `write_attachment` の出力ファイル名を `<attachment_id>-<safe name>`
  に変更（同じメッセージ内で同名の添付が上書きし合わないように）
- `apps/desktop/src-tauri`: `sync.rs` が独自キーを組み立てるのをやめ、
  `Store::set_account_synced_at` を直接使うように統一
- `crates/mailstore`: `upsert_task` の `UPDATE` から `created_by` を除外し、
  人間が作った（`created_by = 'user'`）タスクを Claude が `upsert_tasks` で
  更新しても帰属が `'ai'` に変わらないようにした
- `crates/mailmcp`: 全ツールの description 末尾の安全文言を
  `SAFETY_NOTE_JA` / `SAFETY_NOTE_EN`（`lib.rs`）と `safety_note_ja!` /
  `safety_note_en!`（`main.rs` のマクロ）から `concat!(...)` で組み立てるように統一。
  両者が一致することをテストで固定
- `apps/desktop/scripts/copy-mcp-sidecar.mjs`: `--target <triple>` /
  `--target=<triple>` と `CARGO_TARGET_DIR` に対応。フラグ・環境変数が
  無いときの挙動は変わらない。純粋なパス解決を `resolveSidecarPaths` として
  切り出し、専用のテストファイルを追加

**MCP の使い勝手**
- `search_messages` に `include_body`（既定 false）。true のとき各結果に
  `body_text` の先頭 2,000 文字を付ける
- `inbox_digest` に `limit`（既定 20、最大 50）
- `create_draft` に `reply_all`（既定 false）。true のとき、元メールの
  差出人 + to + cc から自分のアドレスを除いた宛先にする（`to` を明示すればそちらが優先）
- `get_message` の `include_html` を実装（積み残しだった。true のとき `body_html` を返す）
- `upsert_tasks` の `TaskInput.account_id` を省略可にし、省略時は
  `source_message_id` から対応するメールのアカウントを推定する
  （両方無いタスクはエラーにする）
- 全ツールのエラーメッセージを日英併記（`{ja} / {en}`）に統一（`bilingual()` ヘルパー）
- `apps/desktop`: 設定モーダルに 60 秒自動更新（`useAutoRefresh`）の on/off トグルを追加
  （既定オフのまま）

**プロセスの教訓の反映**
- `docs/pr/TEMPLATE.md` を新設。PR 説明の雛形に「実クライアントから呼んで確認したか」
  「秘密情報のgrep」「スクリーンショットの実データ混入」の 3 点チェックリストを含めた
- `CLAUDE.md` に、MCP のツールを変えたら実クライアントから呼んで確かめること、
  作業中は `git stash` を使わないこと、PR は `docs/pr/TEMPLATE.md` を使うことを追記
- `docs/DESIGN.md` §6 の MCP ツール一覧表と `docs/adr/0007-mcp-server.md` に、
  上記の引数追加・エラーメッセージ・安全文言マクロ化を追記

## Why

- **タスクの原子性** — `upsert_tasks` は複数タスクをまとめて渡す設計（CLAUDE.md の
  「1 通ずつ取らせない」と同じ発想）。途中の 1 件が不正で失敗すると、それより前の
  タスクだけ書き込まれた中途半端な状態になり、Claude も人間も気づけない。検証を
  全件先に済ませてから 1 トランザクションで書くことで、成功か失敗かのどちらかにする
- **添付の同名衝突** — 1 通のメールに同名の添付が複数あるケース（例: 複数の `image.png`）で
  上書きが起きていたため、`attachment_id` を前置してファイル名を一意にした
- **最終同期時刻キーの一本化** — `sync:<account_id>:finished_at` というキー形式は
  MCP からも直接読まれる（別プロセスなので GUI のメモリを読めない、ADR 0007 決定 6）。
  組み立て方が 2 箇所にあると片方だけ直して食い違う事故が起きるため、
  `Store::set_account_synced_at` に一本化した
- **`created_by` の据え置き** — `status` を上書きしない（ADR 0007 決定 7）のと同じ理由で、
  `created_by` も人間が作ったタスクの帰属を Claude 経由の更新で変えてはいけない
- **description の安全文言の定数化** — 手で 11 個のツールに同じ文言を貼っていくと、
  1 つだけ書き換え漏れが起きるリスクがある。定数・マクロから組み立ててテストで
  固定することで、文言のドリフトを防ぐ
- **MCP の引数追加** — いずれも「Claude に往復させない」という ADR 0007 の方針の延長。
  `include_body` は検索結果を見てもう一度 `get_message` を呼ぶ往復を減らし、
  `reply_all` は全員返信のたびに宛先一覧を Claude 側で組み立て直す必要をなくす
- **エラー日英併記** — Claude Desktop / Claude Code は英語で動くこともあるため、
  日本語だけのエラーでは文脈が伝わらないことがあった
- **`--target` 対応** — CI やクロスビルドで `cargo build --target <triple>` を使う場合、
  既定の `target/release` ではなく `target/<triple>/release` に成果物ができるため、
  sidecar コピーがそれを追えるようにした

## ツール一覧 / 変更点

引数・返り値が変わった 5 ツールのみ抜粋（他 6 ツールは変更なし）。

| ツール | 変更点 |
|---|---|
| `search_messages` | `include_body`（既定 false）追加。true で各結果に `body_text` の先頭 2,000 文字 |
| `inbox_digest` | `limit`（既定 20、最大 50）追加 |
| `get_message` | `include_html` を実装（`body_html` を返す。無ければ null） |
| `upsert_tasks` | `TaskInput.account_id` が省略可に。省略時は `source_message_id` から推定 |
| `create_draft` | `reply_all`（既定 false）追加。true で元メールの to/cc も宛先に含める |

## テストの状況

各コミットで Rust 側の単体テスト（`crates/mailmcp/src/main.rs` の `mod tests`、
`crates/mailstore/src/lib.rs` の `mod tests`）と `crates/mailmcp/tests/stdio.rs` の
統合テストを追加・更新している（`include_body` / `limit` / `reply_all` /
`include_html` / `account_id` 推定 / 安全文言の一致など、それぞれにテストがある）。
`apps/desktop/scripts/copy-mcp-sidecar.test.mjs` を新設し、`--target` /
`CARGO_TARGET_DIR` のパス解決を確認している。`SettingsModal.test.tsx` に
60 秒自動更新トグルのテストを追加している。

このドキュメントだけの単位（CLAUDE.md / DESIGN.md / ADR / PR 雛形）ではビルド・
テストの再実行はしていない。実際の `cargo test --workspace` / `pnpm test` の
実行結果は、コードを変更した各コミット・PR の最終レビュー時点のものを参照すること。

## レビューで直したもの

PR #4 のレビューで積み残しにした 6 件（上記「PR #4 の積み残し 6 件」参照）。
このドキュメント単位自体へのレビュー指摘は無し。

## 積み残し

| 項目 | 送り先 |
|---|---|
| MCP から GUI への push 通知は無く、フォーカス再読込＋既定オフの 60 秒ポーリングで代用 | 未定 |
| 要約・タスク抽出・下書き・承認送信を実データに繋ぐ | P3-b |
| Gmail / Microsoft 365 | P4 |

## 関連 ADR

- [ADR 0007: MCP サーバの作り方](../adr/0007-mcp-server.md)（追記 2 件をこの PR で追加）

## チェックリスト

- [x] MCP のツールを足す / 返り値を変えたときは、**実クライアント**（Claude Code か Claude Desktop）から
      実際に呼んで schema validation を通した
- [x] スクリーンショットに実在のメール・アドレス・社名が写っていない（モックデータのみ）
- [x] 秘密情報が入っていない（`git diff` に対してパスワード・トークン・接続設定の grep をかけた）
