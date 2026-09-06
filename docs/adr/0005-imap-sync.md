# 5. IMAP 同期の作り方（P0-b）

- 状態: 採用
- 日付: 2026-09-04

## 背景

P0-b で汎用 IMAP 1 アカウントの INBOX（直近 90 日）を SQLite に入れる。
`MailBackend` トレイトは骨だけあり、`imap` / `parse` / `engine` は未実装。
実アカウントに繋ぐので、認証情報とメール本文の置き場所を先に決める必要がある。

## 決定

**クレート** — `async-imap`（TLS 993 と STARTTLS 143 の両対応）は既定の
`runtime-async-std` を切り `runtime-tokio` フィーチャを有効にすることで
tokio の AsyncRead/AsyncWrite とそのまま噛み合わせる（`tokio-util` の
`compat()` は使わない）。TLS は `tokio-rustls` ＋ `webpki-roots`。
`tokio-rustls` は既定の暗号バックエンド（`aws-lc-rs`）ではなく
`default-features = false, features = ["ring", "tls12", "logging"]` で `ring` を使う。
パースは `mail-parser`（日本語エンコーディングを内包するので
`encoding_rs` は足さない）。秘密情報は `keyring`、対話入力は `rpassword`。

**`MailBackend` を 2 点広げる**（`mailcore`）。UIDVALIDITY と 90 日の窓を
バックエンドの外から扱えるようにするため:

- `folder_status(folder) -> FolderStatus { uidvalidity, uid_next }` を追加
- `fetch_new(folder, since_uid, since: Option<DateTime<Utc>>)` に日付の窓を渡す

**同期手順**（`SyncEngine::sync_once`）— `folder_status` → 保存済み `uidvalidity` と
比較し、変わっていれば `last_uid = 0` に戻して取り直す → `UID SEARCH UID (last+1):*
SINCE <90日前>` → 200 件ずつ `UID FETCH (UID FLAGS RFC822)` → raw を
`<data_dir>/mail/<account_id>/<folder>/<uid>.eml` に保存（P3-a でデータ置き場を
アプリデータディレクトリ配下に移した）→ `parse` → `insert_message`。
**1 通のパース失敗で全体を止めない**。`SyncReport { fetched, inserted, skipped, errors }`
に数えてログに残し、次の UID へ進む。

**秘密情報** — パスワードは `keyring::Entry::new("meowbox", "account:{id}")` だけに置く。
`settings_json` に入れるのは `host` / `port` / `username` / `starttls` のみ。
`.env` や設定ファイルからパスワードを読む経路は作らない。`RawMessage.raw` と
パスワードを `tracing` に出す経路も作らない（進捗ログは件数だけ）。

**本文** — `body_text` は引用（`>` 行、`On ... wrote:`、`----- Original Message -----`、
`----- 元のメッセージ -----`）と署名（`-- ` 以降）を落とした要約向けテキスト。
`body_html` は HTML パートがあるときだけ入れる。text/plain が無ければ HTML から
タグを剥がして `body_text` にする。

**添付** — P0-b では `attachments` にメタデータ（filename / mime / size）だけ入れ、
`path` は NULL のまま。本体は raw `.eml` の中に残す。展開は P3。

## 理由

- UIDVALIDITY をバックエンドに閉じ込めると、engine 側が「取り直すべきか」を
  判断できない。トレイトを広げる方が、Gmail / Graph を足すときも同じ形で済む
- パース失敗で同期全体が止まると、1 通の変なメールでアカウントごと同期不能になる。
  実アカウントでは必ず変なメールが来る前提で作る
- 引用と署名を落とした本文を持つと FTS の精度と要約の質が上がる。落とした分が
  必要になったら `raw_path` の `.eml` を読み直せばよく、DB に二重に持たなくてよい
- `tokio-rustls` の既定 `aws-lc-rs` は cmake と nasm を要求し CI（windows/ubuntu）で
  ビルドが落ちる。`ring` は追加ツールなしでビルドできるため優先した

## 結果

- スキーマ変更なし（`folders.uidvalidity` と `attachments` は v1 で用意済み）。
  `SCHEMA_VERSION` は 1 のまま
- `mailstore` に `set_folder_uidvalidity` / `folder_uidvalidity` /
  `reset_folder_uid` / `insert_attachment_meta` を足す
- ネットワークを使うテストは `#[ignore]`。`MEOWBOX_TEST_IMAP_*` があるときだけ走らせる。
  CI では走らない。通常のテストは人工の `.eml` フィクスチャとフェイク `MailBackend` で回す
- **P3 への申し送り**: UI の「引用 N 行を表示」は DB に引用文が無いので、
  `get_message` が `raw_path` を読み直して組み立てる。列を足すかはそのとき決める
- IDLE と 90 日より前のバックフィルは P4 のまま
- 1 通の保存/パースに失敗すると、その UID は raw `.eml` としては残るが DB には
  入らず、`last_uid` は後続の成功した UID に追い越されるため再取得もされない
  （poison message でフォルダ全体の同期が止まる方を避けるため意図的）。
  失敗 UID の記録と再インデックスは P4 で扱う
