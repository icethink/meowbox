# P0-b: mailsync による IMAP 同期

## What

`mailsync` の `imap` / `parse` / `engine` を実装し、`meowbox sync` で実 IMAP アカウントの
INBOX（直近 90 日）が SQLite に入り、`meowbox search` で日本語の全文検索が当たるようになった。

- **`ImapBackend`**（async-imap）— 暗黙 TLS 993 と STARTTLS 143 の両対応、
  SPECIAL-USE 属性 + フォルダ名からの role 推定、UID + SINCE による差分取得、
  200 件ずつ `UID FETCH`
- **`parse`**（mail-parser）— ISO-2022-JP / Shift_JIS / UTF-8、multipart/alternative、
  添付メタデータ、引用と署名を落とした `body_text`
- **`SyncEngine`** — UIDVALIDITY の変化を検知して取り直し、raw `.eml` の保存、
  1 通・1 フォルダの失敗で全体を止めない、`SyncReport` で件数を返す
- `normalize_subject` の拡張（`Re[2]:` `Re(3):`、全角コロン・全角英字の RE / FW など）
- CLI（`accounts set-password` / `sync` / `show`）

## Why

- **秘密情報を keyring だけに置いた理由** — パスワードが DB やログ、`settings_json` に
  残ると、DB ファイルや設定のバックアップを渡しただけで漏れる。`settings_json` には
  接続先（`host` / `port` / `username` / `starttls`）だけを入れ、パスワードは
  `keyring::Entry::new("meowbox", "account:{id}")` の 1 箇所からしか読み書きしない
  経路にした。
- **`MailBackend` に `folder_status` を足し、`fetch_new` に日付の窓を渡すようにした理由** —
  UIDVALIDITY が変わったかどうかは「保存済みの値と今回の値を比べる」という
  ステートフルな判断で、バックエンドの中に閉じ込めると engine 側が
  「取り直すべきか」を知る手段がなくなる。判定を `SyncEngine::sync_once` に
  寄せることで、Gmail / Graph バックエンドを足すときも同じ形で扱える。
- **1 通の失敗で同期全体を止めない理由** — 実アカウントには必ず壊れたメールが
  混ざる。1 通のパース失敗でアカウントごと同期不能になるのを避け、
  `SyncReport { fetched, inserted, skipped, errors }` に件数として残して
  次の UID へ進むようにした。

## 既知の差分・制限

| 項目 | 内容 |
|---|---|
| 失敗した通の扱い | raw `.eml` には残るが DB には入らず、再取得もされない（P4 で再インデックス） |
| 添付の展開 | メタデータ（filename / mime / size）のみ。`attachments.path` は NULL のまま。本体の展開は P3 |
| IDLE / 過去分バックフィル | どちらも P4 |
| `mail-parser` の寛容さ | 壊れたバイト列でもパースが成功することがある。`errors` に数えられるのは主に空・切り詰められた取得 |
| 実接続テスト | `#[ignore]`。`MEOWBOX_TEST_IMAP_*` があるときだけ手元で走る（CI では走らない） |
| UI の「引用 N 行を表示」 | DB に引用文を持たないため、P3 で `raw_path` の `.eml` を読み直す実装が必要 |

## Checklist

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] 実アカウントでの同期・検索・再同期の確認（P3-a（PR #3）の GUI から実施した
      （2026-09-06）。データ置き場がアプリデータディレクトリ配下に移ったため、
      CLI ではなく GUI で確認した）

## Notes

- `tokio-rustls` の暗号バックエンドを既定の `aws-lc-rs` から `ring` に固定した。
  `aws-lc-rs` はビルドに cmake と nasm を要求し CI（ubuntu / windows）で落ちるため。
- `async-imap` は既定の `runtime-async-std` を切り `runtime-tokio` フィーチャを
  有効にすることで tokio の AsyncRead/AsyncWrite にそのまま噛み合った。
  `tokio-util` の `compat()` 層は不要だった（詳細は
  [ADR 0005](../adr/0005-imap-sync.md)）。
