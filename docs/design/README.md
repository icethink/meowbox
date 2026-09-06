# docs/design

Claude Design で作った Meowbox の UI デザイン。

| ファイル | 内容 |
|---|---|
| `main-dark.png` | メイン画面（ダーク）のスクリーンショット。実装時の見本 |
| `tokens.css` | 上記から抽出したデザイントークン。`apps/desktop/src/styles/tokens.css` にそのまま置く想定 |
| `impl-main-dark.png` | 実装の現状のスクリーンショット。`main-dark.png`（見本）に対して実装がどこまで追いついているかを見るためのもの |
| `first-run-empty.png` | 初回起動（アカウント 0 件）の空状態 |
| `wizard-kind.png` | アカウント追加ウィザード ステップ 1（種別） |
| `wizard-server.png` | 同 ステップ 2（サーバー設定・接続テスト成功後） |
| `wizard-project.png` | 同 ステップ 3（案件タグ） |
| `settings.png` | 設定モーダル（アカウントの再同期・削除） |

P3-a 以降のスクリーンショットはすべてモックデータ（`VITE_MEOWBOX_MOCK=1`）で撮影している。
実アカウントで同期した画面はリポジトリに入れない。

Claude Design の元ファイル（.html バンドル）は `メール管理/design/` に置いてある（リポジトリには含めない: 7MB）。

## 実装時の指針
- 人間側の操作（送信・選択・要対応）は `--accent`、Claude 由来（要約・抽出タスク・AI 下書き）は `--ai` を使い、混ぜない
- メールアドレス・数値・時刻・キーボードショートカットは `--font-mono`
- 密度は高め: 基本 12.5px / 行間 1.6、余白は 6〜14px
- 3 ペイン: sidebar 220 / list ~390 / thread 残り / right panel 300（トグル）
- ライトテーマは未デザイン。トークンの命名は保ったまま値だけ差し替える前提
