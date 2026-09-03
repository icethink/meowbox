# Meowbox — 設計メモ (v0.1 / 2026-09-02)

## 1. 目的
- 案件ごとに配布されるメールアドレスを **1つのアプリで一元管理** する
- Claude（Desktop / Claude Code / Cowork）から **MCP経由で直接操作できる** メーラーにする
- 要約・タスク抽出・返信下書きをAIに任せ、人間は確認と送信判断だけ行う
- Thunderbird MCP のような「他アプリ経由の不安定な連携」を排除し、自前で経路を持つ

## 2. 決定事項
| 項目 | 決定 |
|---|---|
| 名前 | **Meowbox**（Mailbox × にゃー）。crate / CLI 名は `meowbox` |
| 言語 | Rust |
| UI | Tauri v2（フロントは React + TypeScript を想定。Svelte でも可） |
| Claude連携 | アプリ内蔵の MCP サーバ（主軸）。保険として SQLite/.eml をファイルとしても読める構造にする |
| MVP範囲 | 同期・検索・要約・タスク抽出 ＋ 返信下書き生成（**自動送信はしない**。送信は必ずユーザー操作） |
| アカウント | 汎用 IMAP/SMTP、Gmail/Google Workspace (OAuth2)、Microsoft 365 (OAuth2) |
| リポジトリ | github.com/icethink/meowbox |

## 3. アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│ Tauri v2 App (single binary)                                │
│                                                             │
│  ┌──────────────┐   invoke   ┌────────────────────────────┐ │
│  │ WebView UI   │◄──────────►│ Core (Rust crate: mailcore)│ │
│  │ React + TS   │   events   │  - AccountManager          │ │
│  └──────────────┘            │  - SyncEngine (IMAP IDLE)  │ │
│                              │  - Store (SQLite + FTS5)   │ │
│  ┌──────────────┐            │  - Draft/Outbox            │ │
│  │ MCP Server   │◄──────────►│  - AI jobs (summary/tasks) │ │
│  │ stdio + HTTP │            └────────────────────────────┘ │
│  └──────┬───────┘                        │                  │
└─────────┼────────────────────────────────┼──────────────────┘
          │                                │
   Claude Desktop / Code / Cowork     IMAP / SMTP / Gmail API / Graph API
```

### ワークスペース構成（cargo workspace）
```
meowbox/
├── crates/
│   ├── mailcore/      # ドメインロジック。UIにもMCPにも依存しない
│   ├── mailstore/     # SQLite スキーマ・マイグレーション・FTS
│   ├── mailsync/      # IMAP/SMTP/Gmail/Graph の各バックエンド
│   ├── mailmcp/       # MCP サーバ実装（rmcp）
│   └── mailcli/       # デバッグ用CLI `meowbox`（UIなしで同期・検索できる）
└── apps/
    └── desktop/       # Tauri v2 アプリ（src-tauri + src）
```
mailcore を UI から切り離しておくことで、**UIが死んでいても CLI と MCP は動く** 状態にする。

## 4. 主要クレート候補
| 用途 | クレート | メモ |
|---|---|---|
| IMAP | `async-imap` + `tokio` | IDLE 対応。OAuth2 は XOAUTH2 SASL |
| SMTP | `lettre` | STARTTLS / XOAUTH2 対応 |
| MIME解析 | `mail-parser` | 日本語(ISO-2022-JP, Shift_JIS)のデコードが強い |
| MIME生成 | `mail-builder` | lettre と併用 |
| DB | `rusqlite` (bundled, FTS5有効) | 全文検索は FTS5 + trigram tokenizer で日本語対応 |
| OAuth2 | `oauth2` + ループバック受信 | Google / Microsoft のPKCEフロー |
| 秘密情報 | `keyring` | OS の資格情報ストアにトークン保存 |
| MCP | `rmcp`（公式Rust SDK） | stdio と Streamable HTTP の両対応 |
| Claude API | `reqwest` + 自前クライアント | 要約・タスク抽出をアプリ側で走らせる場合 |
| Gmail API | `reqwest` 直叩き | ラベル・スレッドIDが欲しい場合のみ。基本は IMAP で統一 |
| Graph API | `reqwest` 直叩き | M365 は IMAP OAuth が制限されがちなので Graph 優先 |

## 5. データモデル（SQLite）
正は `crates/mailstore/src/schema.sql`。概要:
```sql
accounts(id, name, kind[imap|gmail|m365], email, project_tag, settings_json, created_at)
folders(id, account_id, path, role[inbox|sent|drafts|trash|archive|other], uidvalidity, last_uid)
messages(id, account_id, folder_id, uid, message_id, thread_key,
         from_addr, from_name, to_json, cc_json, subject, date,
         snippet, body_text, body_html, has_attachments, is_read, is_flagged,
         raw_path)                      -- raw .eml はファイル保存
attachments(id, message_id, filename, mime, size, path)
ai_summaries(id, target["message:<id>"|"thread:<key>"|"daily:<date>"], model, summary, created_at)
tasks(id, account_id, source_message_id, title, due, status[open|done|dismissed],
      confidence, created_by[ai|user], created_at)
drafts(id, account_id, in_reply_to, to_json, subject, body, status[draft|approved|sent], created_at)
messages_fts(subject, body_text, from_name, from_addr)  -- FTS5 trigram, external content
```
- `project_tag` で案件ごとの束ね方を表現（1案件=1アドレスとは限らないので、複数アカウントを同じタグにできる）
- raw `.eml` を `data/mail/<account>/<folder>/<uid>.eml` に保存 → MCPが落ちていても Claude がファイルとして読める保険

## 6. MCP ツール設計（Claude から見える面）
最初から「AIが使いやすい粒度」で切る。1通ずつ取らせない。引数の型は `crates/mailmcp/src/lib.rs`。

| ツール | 役割 |
|---|---|
| `list_accounts` | アカウント・案件タグ一覧 |
| `search_messages(query, account?, project?, since?, unread_only?, limit)` | FTS + 条件検索。返り値は軽量（id/件名/差出人/日付/snippet） |
| `get_thread(thread_key)` | スレッドを時系列で1発取得（本文はテキスト整形済み、引用部は折り畳み） |
| `get_message(id, include_html?)` | 単体取得 |
| `get_attachment(id)` | ファイルパスを返す（Claude 側で Read できる） |
| `inbox_digest(project?, since)` | 未処理メールの要約用ビュー：スレッド単位でまとめ、既存要約があれば添付 |
| `save_summary(target, text)` | Claude が作った要約を DB に保存 |
| `upsert_tasks(tasks[])` | タスク抽出結果を保存 |
| `list_tasks(status?, project?)` | タスク一覧 |
| `create_draft(in_reply_to, body, to?, subject?)` | 返信下書きを保存（**送信はしない**） |
| `mark(ids[], read|unread|archive|flag)` | 状態操作 |

送信系は MVP では MCP に**出さない**。UIの「承認して送信」ボタンだけが送れる。

## 7. 同期エンジン
- アカウントごとに tokio タスクを1本。INBOX は IMAP IDLE で即時反映、他フォルダは 5〜15分ポーリング
- `UIDVALIDITY` 変化検知で再同期。差分は `UID > last_uid` + FLAGS 変更を `SEARCH SINCE` で拾う
- 初回は直近90日→バックグラウンドで過去分を遡る（UIが待たされない）
- 取得した raw をパースして `messages` へ。本文テキストは引用・署名を除いた `body_text` も持つ（要約精度が上がる）
- スレッド判定: `References` 先頭 → `In-Reply-To` → 正規化件名（`mailcore::normalize_subject`）

## 8. AI パイプライン（アプリ側で回す場合）
1. 新着スレッドを検知 → `inbox_digest` と同じビューを作成
2. Claude API に「要約 + タスク候補(JSON)」を1回で依頼（structured output）
3. `ai_summaries` / `tasks(created_by=ai, confidence)` に保存
4. UI では confidence 低いタスクは「候補」として薄く表示、ユーザーが確定
- Cowork / Claude Code から MCP 経由で同じことをやる場合は 6 のツールだけで完結する（API キー不要）
- どちらも動くようにしておき、**「アプリが勝手に要約する」はオプトイン**にする（コストと誤要約の管理のため）
- 実装順は MCP 経由（Claude 側が実行）を先に入れ、アプリ内で Claude API を叩く方式は後日オプトインで追加する

## 9. 認証まわり
- 汎用 IMAP: パスワードは `keyring` に保存。設定 UI で手入力 + 自動検出（Mozilla autoconfig DB を参考）
- Gmail: OAuth2 PKCE。スコープは `https://mail.google.com/`（IMAP XOAUTH2 用）。ユーザー自身の GCP プロジェクトでクライアントIDを作ってもらう（配布用に検証を通す必要がないため）
- Microsoft 365: OAuth2 PKCE。Graph の `Mail.ReadWrite` `Mail.Send`。テナントによっては管理者同意が要る点を UI で案内

## 10. ロードマップ
| フェーズ | 内容 | 状態 |
|---|---|---|
| P0-a | workspace 雛形、mailstore スキーマ、`meowbox init / accounts / search` | ✅ 2026-09-02 |
| P0-b | mailsync で IMAP 同期 → SQLite。手元の IMAP アカウントで動作確認 | ✅ 2026-09-04 |
| P1 | mailmcp: `list_accounts` `search_messages` `get_thread` `inbox_digest`。Cowork から叩けることを確認 | ここで Thunderbird MCP を卒業 |
| P2 | Tauri UI: アカウント一覧・スレッド表示・検索・タスク一覧 | ✅ 2026-09-03 |
| P3 | 要約・タスク抽出（MCP経由とアプリ内API呼び出しの両方）、`create_draft`、UI で承認送信 | MVP 完成 |
| P4 | Gmail / M365 OAuth、Graph バックエンド、IMAP IDLE、過去分バックフィル | 案件アドレス増加に耐える |
| P5 | 案件タグ横断ダッシュボード、日次ダイジェスト、Thunderbird からのインポート | 便利機能 |

## 11. 未決事項
- フロントエンドを React か Svelte か（Claude Code で量産するなら React の方が事例が多い）
- Windows 以外（Mac/Linux）も最初から対象にするか
- 要約に使うモデルとコスト上限（日次でいくらまで、など）
- 案件終了後のアカウントの扱い（アーカイブして DB に残す／削除）
