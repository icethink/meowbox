# Architecture Decision Records

後から「なぜこうなっているのか」を追えるように、方針を決めたときだけ 1 ファイル足す。
書式は [MADR](https://adr.github.io/madr/) を簡略化したもの（背景 / 決定 / 理由 / 結果）。

| # | 決定 |
|---|---|
| [0001](0001-tauri-v2.md) | デスクトップ UI に Tauri v2 を使う |
| [0002](0002-no-send-over-mcp.md) | 送信ツールを MCP に出さない |
| [0003](0003-design-tokens-as-single-source.md) | 色・字・角丸の出所をデザイントークンだけにする |
| [0004](0004-accent-and-ai-two-colour-rule.md) | 人間由来は accent、Claude 由来は ai。2 色を混ぜない |
| [0005](0005-imap-sync.md) | IMAP 同期の作り方（接続・UIDVALIDITY・秘密情報・本文の持ち方） |

決定を覆すときは、元のファイルを消さずに状態を「置き換え済み（→ NNNN）」にして
新しい番号で書く。
