//! mailsync — 各バックエンドから raw メールを取得し、パースして mailstore に入れる。
//!
//! 構成:
//! - `imap`   : 汎用 IMAP（パスワード / XOAUTH2）。P0 の実装対象。
//! - `parse`  : RFC822 → `NewMessage` 変換（mail-parser を使う予定）。
//! - `engine` : アカウントごとの同期ループ（差分 UID、IDLE、バックフィル）。
//!
//! 現状はスケルトン。`MailBackend` トレイトは mailcore にある。

pub mod engine;
pub mod imap;
pub mod parse;
