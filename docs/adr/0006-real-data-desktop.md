# 6. 実データを扱うデスクトップアプリ（P3-a）

- 状態: 採用
- 日付: 2026-09-06

## 背景

P0-b までで CLI からの IMAP 同期は動くが、使うには CLI とコマンドラインの知識が要る。
P3-a の目標は「インストールして起動 → 画面からアカウント登録 → 同期 → 自分のメールが見える」を
アプリだけで完結させること。そのために置き場所・秘密情報・同期の進捗の 3 つを先に決める必要がある。

## 決定

**1. データの置き場** — `mailstore::paths` に集約。既定は OS のアプリデータディレクトリ配下
（Windows `%APPDATA%\dev.icethink.meowbox\`、macOS `~/Library/Application Support/…`、
Linux `~/.local/share/…`）。配下に `meowbox.db` / `mail/` / `attachments/`。
`MEOWBOX_DATA_DIR` で丸ごと上書きできる。デスクトップは Tauri の `app_data_dir()` を、
CLI は `dirs::data_dir()/APP_DIR_NAME` を使うが、identifier（`dev.icethink.meowbox`）が
同じなので同じ場所を指す。CLI の `--db` 未指定時もここを見る。P0-b までに CLI で作った
`./data/meowbox.db` は引き継がない（ウィザードで登録し直す）。

**2. 秘密情報** — パスワードは Rust 側で `keyring` にだけ入れる。JS の state・localStorage・
ログ・Tauri のイベントペイロードには載せない。`invoke` の引数として 1 回渡して終わり。
`settings_json` に入るのは host / port / username / starttls だけ。IMAP の認証失敗は
サーバ応答をそのまま UI に返さず固定の日本語文言にする（応答にパスワードが混ざる経路を
作らないため）。アカウント削除時は keyring のエントリも消す。

**3. 同期の呼び方と進捗** — `sync_account(id)` はバックグラウンドタスクを起こしてすぐ返る。
進捗は `sync://progress` イベントで逐次流す。ペイロードは
`{ account_id, folder, fetched, total, inserted, errors, done, error }` の 8 つだけで、
件数とフォルダ名しか載せない。`total` は「同期中 120 / 300」の分母として足した。
`done = false` の件数はフォルダ単位、`done = true` は全体合計。同期が失敗する経路では
エンジンが `done` を出さないので、コマンド側が `done = true, error = Some(...)` を 1 回出す。
同じアカウントの多重実行はガードで拒否する。P3-a で同期するのは INBOX の直近 90 日だけ
（Sent / Trash まで取ると一覧に自分の送信メールが混ざるため）。他フォルダと IDLE は P4。

**4. 引用文** — DB に列を足さない。`get_message` が `raw_path` の `.eml` を読み直し、
`mailsync::parse::split_quotes_and_signature` で引用部を組み立てて返す
（ADR 0005 の申し送りへの回答）。

**5. 添付** — 遅延展開。クリックされたときだけ `extract_attachment(id)` が `.eml` から
取り出して `<data_dir>/attachments/<message_id>/<filename>` に書き、パスを返す。
ファイル名は `.eml` 由来＝信頼境界の外なので `mailsync::fsname::sanitize_path_segment` を
通す。`attachments.path` に記録して 2 回目以降は再利用する。

**6. アーカイブ列（スキーマ v2）** — `mark(archive)` の状態を持つ場所が無かったので
`messages.is_archived` を足し、`SCHEMA_VERSION` を 2 に上げた。v1 の DB には
`ALTER TABLE` で列を足す。IMAP には書き戻さない（DB のみ。既読/フラグも同じ）。TODO(P4)。

**7. 表示用の値は UI 側で作る** — `time_label` のような表示専用の文字列は Rust 側では
作らない。API は RFC 3339 の日時だけを返し、`lib/relativeDate.ts` が組み立てる。
「今日」の境界も UI がローカルの 0:00 を絶対時刻にして渡す（Rust 側は「今日」を知らない）。

**8. AI 由来の表示** — 要約・抽出タスク・ダイジェストの実データはまだ無い（P1 で MCP 経由に
Claude が書き込む）。UI はモックの要約文を実データに混ぜず、`--ai` 色の薄い空状態を出す。

## 理由

- データの置き場を Tauri と CLI で共有する identifier に揃えたのは、同じマシンで
  `meowbox sync` と GUI を両方使っても別々の DB を見てしまう事故を防ぐため
- パスワードを keyring に閉じ込め、`settings_json` から追い出したのは ADR 0005 の方針の
  延長。IMAP の認証エラーを固定文言にするのも、サーバ応答の文字列にパスワードや
  資格情報の断片が混ざって UI やログに漏れる経路を最初から作らないため
- 進捗をイベントにしたのは、同期は数十秒かかる処理で、`invoke` の戻りを待たせると
  UI が固まって見えるため。件数とフォルダ名だけに絞ったのは、進捗イベントも
  「秘密情報を渡さない経路」であるべきという 2 の方針と同じ理由
- 引用文を DB に持たないのは ADR 0005 の理由（FTS と要約の質を上げるために本文からは
  落とすが、必要になったら `.eml` を読み直せば済み、二重に持たなくてよい）をそのまま
  引き継いだもの。UI 都合の「引用 N 行を表示」のためだけに列を増やしたくない
- 添付を遅延展開にしたのは、同期のたびに全添付を書き出すとディスクと同期時間を
  無駄に食うため。開かれた添付だけを都度取り出す方が実際の使われ方に合う
- INBOX だけを同期するのは、Sent / Trash まで取り込むと自分が送ったメールがスレッドや
  一覧に混ざり、「受信メールを見る」という P3-a の最初のゴールがぼやけるため
- 表示用ラベルを Rust で作らないのは、ロケール・タイムゾーン・「今日」の境界の判断が
  UI の都合であり、Rust 側に持たせると MCP 経由で Claude が同じ関数を使うときに
  ローカル時刻の前提が邪魔になるため

## 結果

- スキーマ v2: `messages.is_archived` を追加。`Store::migrate` が `schema_version` を見て
  v1 の DB には `ALTER TABLE` を、新規 DB には `schema.sql` をそのまま適用する
- `mailstore::paths` を新設（`default_data_dir` / `resolve_data_dir` / `db_path` /
  `mail_dir` / `attachments_dir` / `ensure_data_dir`）。`mailsync::engine::SyncOptions` の
  既定 `data_dir` もここを参照するよう変更
- `mailsync::engine::SyncOptions` に `progress: Option<ProgressSink>` を追加し、
  `SyncProgress` をフォルダ単位・全体合計の両方で通知する
- `mailsync::imap::ImapBackend::with_password` を追加（アカウント保存前の接続テスト用）。
  `ImapConfig::from_account` を `mailcore::Account` から組み立てる関数として追加
- `mailsync::fsname::sanitize_path_segment` を新設し、IMAP フォルダ名・添付ファイル名
  など信頼境界の外から来る文字列をパスの 1 セグメントとして安全に使えるようにした
- `apps/desktop/src-tauri` に `AppState` / `commands::accounts` / `commands::sync` を追加。
  `list_accounts` / `add_account` / `set_account_password` / `test_connection` /
  `delete_account` / `list_projects` / `sync_account` / `sync_status` を実装
- **P1 への申し送り**: MCP サーバを Tauri アプリの中でどう起動するか（stdio か HTTP か）、
  Claude Desktop / Cowork からどう見つけさせるかは未決。DB を UI・同期タスク・MCP の
  3 者が触ることになるので、SQLite の同時アクセス（WAL）と書き込みの直列化をそのとき
  確認する必要がある
- 積み残し: 送信は今回も作らない、IMAP への書き戻し（既読/フラグ/アーカイブ）は P4、
  Gmail / M365 は「近日対応」の無効表示だけ、署名・自動更新はやらない
