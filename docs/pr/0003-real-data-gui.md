# P3-a: 実データを扱うデスクトップアプリ

## What

`apps/desktop/src/api/` のモックを Tauri の `invoke()` に差し替え、CLI も設定ファイルも
触らずに「インストールして起動 → 画面からアカウント登録 → 同期 → 自分のメールが
一覧・スレッドで見える」までをアプリだけで完結させた。

- **データの置き場**（`mailstore::paths`）— OS のアプリデータディレクトリ配下に集約し、
  `MEOWBOX_DATA_DIR` で丸ごと上書きできるようにした。デスクトップと CLI が
  identifier（`dev.icethink.meowbox`）を共有し、同じ DB を見る
- **スキーマ v2** — `messages.is_archived` を追加。`Store::migrate` が
  `schema_version` を見て v1 の DB には `ALTER TABLE` を、新規 DB には
  `schema.sql` をそのまま適用する
- **mailstore の読み取り API** — スレッド一覧・未読件数・アーカイブ状態、
  アカウントの検索・削除、タスク・要約・添付・meta の読み書き、下書きの
  挿入・一覧
- **mailsync の拡張** — 同期進捗をコールバックで通知（`SyncOptions.progress`）、
  引用文と署名の分離、添付本体の遅延展開、`ImapConfig::from_account` /
  `ImapBackend::with_password`（アカウント保存前の接続テスト用）、
  `fsname::sanitize_path_segment`（フォルダ名・添付ファイル名を安全にパスへ使う）
- **Tauri コマンド**（`apps/desktop/src-tauri`）— `AppState` を新設し、
  `list_accounts` / `add_account` / `set_account_password` / `test_connection` /
  `delete_account` / `list_projects` / `sync_account`（`sync://progress` イベント）/
  `list_threads` / `get_thread` / `get_message` / `extract_attachment` /
  `get_digest` / `mark` / `create_draft` / `list_drafts` を実装
- **UI** — 初回起動の空状態、アカウント登録ウィザード（種別 → サーバー設定 +
  接続テスト → 案件タグ）、実データのスレッド・一覧表示（相対時刻・引用の
  折りたたみ）、添付を開く、返信下書きの保存、要約・タスクが無いときの
  AI 空状態、歯車から開く設定モーダル（アカウントごとの再同期・削除）
- **表示用の日時**（`src/lib/relativeDate.ts`）— API は RFC 3339 の日時だけを返し、
  相対時刻と「今日」の境界は UI 側で組み立てる

## Why

[ADR 0006](../adr/0006-real-data-desktop.md) を参照。要点だけ再掲する。

- データの置き場を Tauri と CLI で共有する identifier に揃えたのは、同じマシンで
  `meowbox sync` と GUI を両方使っても別々の DB を見てしまう事故を防ぐため
- パスワードは Rust 側で `keyring` にだけ入れ、JS の state・localStorage・ログ・
  イベントペイロードには載せない。IMAP の認証失敗は固定の日本語文言にして、
  サーバ応答の文字列がそのまま UI に出る経路を作らない
- 同期の進捗をイベント（`sync://progress`）にしたのは、同期は数十秒かかる処理で
  `invoke` の戻りを待たせると UI が固まって見えるため。ペイロードは件数と
  フォルダ名だけに絞った
- 引用文を DB に持たせず、`get_thread` が `raw_path` の `.eml` を読み直して
  組み立てるようにしたのは、FTS と要約の質を上げるために本文からは落とす
  という ADR 0005 の方針をそのまま引き継いだため
- 添付を遅延展開にしたのは、同期のたびに全添付を書き出すとディスクと同期時間を
  無駄に食うため。開かれた添付だけを都度取り出す
- INBOX だけを同期するのは、Sent / Trash まで取り込むと自分が送ったメールが
  スレッドや一覧に混ざり、「受信メールを見る」という P3-a の最初のゴールが
  ぼやけるため
- 表示用ラベル（相対時刻・「今日」の境界）を Rust 側で作らないのは、
  ロケール・タイムゾーンの判断が UI の都合であり、Rust に持たせると MCP 経由で
  Claude が同じ関数を使うときにローカル時刻の前提が邪魔になるため

## スキーマ v2

`messages.is_archived INTEGER NOT NULL DEFAULT 0` を追加し、`SCHEMA_VERSION` を
1 から 2 に上げた。`Store::migrate` は開いた DB の `schema_version` を見て、

- v1 の DB → `is_archived` 列が無ければ `ALTER TABLE messages ADD COLUMN is_archived
  INTEGER NOT NULL DEFAULT 0` を実行してから `schema_version` を書き換える
- schema_version 行そのものが無い古い DB → v1 として扱ってから同じ経路を通す
  （レビューで見つかった穴。「既知の差分・レビューで直したもの」参照）
- 新規 DB → `schema.sql` をそのまま流す（列は最初から入っている）

IMAP への書き戻しはしない。既読・フラグ・アーカイブはすべてアプリの DB の中だけで
完結する（TODO(P4)）。

## テストの状況

実行して確認した件数（2026-09-06 時点）:

```
cargo test --workspace 2>&1 | Select-String "test result"
```
→ 全 14 クレート/バイナリ分の結果を合計すると **145 passed, 0 failed, 1 ignored**
（ignored は `#[ignore]` の実 IMAP 接続テスト。`MEOWBOX_TEST_IMAP_*` があるときだけ
手元で走る）

```
cd apps/desktop; pnpm test
```
→ **102 passed**（17 ファイル、0 failed）

## レビューで直したもの

- **マイグレーションの穴** — `schema_version` 行が無い（P0-a 以前に作られた）DB を
  開くと `Store::migrate` が失敗していた。v1 として扱ってから v2 への
  `ALTER TABLE` を通す経路を足した（`fix(mailstore): upgrade a database whose
  schema version row is missing`）
- **`SyncOptions::default()` のパス** — 既定値が `./data` を指しており、CLI と
  デスクトップが別々の場所を見てしまっていた。`mailstore::paths::default_data_dir()`
  を参照するように直した（`fix(mailsync): default the sync data dir to the shared
  data directory`）
- **ドキュメントのパス表記** — データディレクトリ移動後、raw `.eml` のパスの記述が
  古いままだった箇所を修正した（`docs: correct the raw eml path after the data
  directory move`）
- **送信していないのに「送信しました」と出ていた** — `api/tauri.ts` の `sendDraft` が
  何もしない実装のまま、`ReplyBox` が無条件に成功トーストを出していた。本番ビルドで
  「確認して送信」を押すと、1 通も送信していないのに成功表示が出る状態だった。
  送信は MVP の安全境界の外（下書きの保存まで）なので、送信ボタンを無効化して
  「送信は未対応です（下書きの保存まで）」と明示し、代わりに「下書きを保存」を追加した
  （`fix(desktop): stop reporting success for mail that was never sent`）
- **実データ経路がモックの作文を返していた** — `api/tauri.ts` の `generateAiDraft` が
  `VITE_MEOWBOX_MOCK` の判定を通らずに `src/mock/drafts.ts` の文面をそのまま返しており、
  本番ビルドで「AI で下書き」を押すとサンプル文面が本文に入る状態だった。
  実データ実装から `src/mock/` への import を切り、使えないことを UI に出すようにした
  （`fix(desktop): stop returning mock prose from the real api path`）

## 積み残し（P3-b 以降）

| 項目 | 送り先 |
|---|---|
| 送信（下書きの保存までで、送信ボタンは無い） | P3-b |
| Gmail / Microsoft 365 | ウィザードで「近日対応」の無効表示のみ。実装は P4 |
| IMAP への書き戻し（既読・フラグ・アーカイブ） | アプリの DB の中だけ。P4 |
| 要約・タスク抽出・ダイジェストの実データ | P1 で Claude が MCP 経由で書き込む |
| MCP サーバの起動方法（stdio / HTTP）、Claude Desktop / Cowork からの発見のさせ方 | P1 |
| 署名・自動更新 | 未着手 |

## Checklist

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`（145 passed / 1 ignored）
- [x] `pnpm lint`
- [x] `pnpm typecheck`
- [x] `pnpm test`（102 passed / 17 ファイル）
- [x] `pnpm build`
- [x] `pnpm tauri dev` で起動し、アカウント登録 → 同期 → 一覧・スレッド表示を確認
- [x] `pnpm tauri build`（NSIS インストーラの生成。`target/release/bundle/nsis/
      Meowbox_0.1.0_x64-setup.exe` と `bundle/msi/Meowbox_0.1.0_x64_en-US.msi` を生成済み）
- [x] クリーンな `MEOWBOX_DATA_DIR` で起動して空状態が出ることを確認
- [x] ADR 0006
- [x] `CLAUDE.md` の P3-a にチェック、P3-b を切り出し
- [x] 実アカウントでの一連の流れの確認（飼育員さんの端末で `pnpm tauri dev` から
      「空状態 → ウィザード → 実アカウント登録 → 同期 → 一覧・スレッド表示」を確認済み
      （2026-09-06）。件数・所要時間などの具体的な数値は後日追記）

## Notes

- スクリーンショット（`docs/design/first-run-empty.png` / `wizard-server.png` /
  `settings.png`）は README の「使い方」節から参照している。すべてモックデータ。
