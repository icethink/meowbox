# Meowbox 🐱📮

**AI-friendly mail aggregator, written in Rust.**

案件ごとに配布されるメールアドレスを 1 つに束ね、Claude が MCP 経由で
検索・要約・タスク抽出・返信下書きまで行えるデスクトップメーラー。

- 複数アカウント（汎用 IMAP / Gmail / Microsoft 365）を案件タグで横断
- アプリ内蔵 MCP サーバ — Claude Desktop / Claude Code / Cowork から直接操作
- SQLite + FTS5（trigram）で日本語全文検索
- 生の `.eml` もファイル保存 — MCP が止まっていても AI がファイルとして読める
- 送信は必ず人間が承認（AI は下書きまで）

## Status

早期開発中。現在 P0（同期エンジン）を実装中。ロードマップは [docs/DESIGN.md](docs/DESIGN.md)。

## Quick start (dev)

```sh
cargo build --workspace
cargo test --workspace
cargo run -p mailcli -- init
cargo run -p mailcli -- accounts add --name work --email you@example.com --project 案件A --host imap.example.com
cargo run -p mailcli -- accounts list
cargo run -p mailcli -- search "見積" --project 案件A
```

## Layout

```
crates/mailcore   domain types & backend trait (no I/O)
crates/mailstore  SQLite storage, schema, FTS
crates/mailsync   IMAP / Gmail / Graph backends, MIME parsing, sync engine
crates/mailmcp    MCP server for Claude
crates/mailcli    `meowbox` debug CLI
apps/desktop      Tauri v2 app (coming in P2)
```

## Development with Claude Code

`CLAUDE.md` に作業ルールと「次の一手」を書いてある。`claude` を起動して
「CLAUDE.md の次のフェーズを進めて」で続きから始められる。

## License

MIT
