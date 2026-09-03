# docs/design

Claude Design で作った Meowbox の UI デザイン。

| ファイル | 内容 |
|---|---|
| `main-dark.png` | メイン画面（ダーク）のスクリーンショット。実装時の見本 |
| `main-dark.reference.html` | 同画面の HTML ソース（インラインスタイル、React 不要で構造が読める）。実装時の寸法・色・余白の参照用 |
| `tokens.css` | 上記から抽出したデザイントークン。`apps/desktop/src/styles/tokens.css` にそのまま置く想定 |

Claude Design の元ファイル（.html バンドル）は `メール管理/design/` に置いてある（リポジトリには含めない: 7MB）。

## 実装時の指針
- 人間側の操作（送信・選択・要対応）は `--accent`、Claude 由来（要約・抽出タスク・AI 下書き）は `--ai` を使い、混ぜない
- メールアドレス・数値・時刻・キーボードショートカットは `--font-mono`
- 密度は高め: 基本 12.5px / 行間 1.6、余白は 6〜14px
- 3 ペイン: sidebar 220 / list ~390 / thread 残り / right panel 300（トグル）
- ライトテーマは未デザイン。トークンの命名は保ったまま値だけ差し替える前提
