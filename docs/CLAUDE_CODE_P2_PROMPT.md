# Claude Code に投げるプロンプト（P2: Tauri UI をデザインから実装）

使い方（Windows ネイティブ）: PowerShell か Git Bash で リポジトリ直下（Source ディレクトリ） に移動 →
`claude` を起動 → 最初に `/design-login` を 1 回実行 → 下の「ここから〜ここまで」をそのまま貼る。
WSL からは使わない（Tauri は Windows 版をビルドしたいため）。

---- ここから ----

これは Meowbox（Rust + Tauri v2 製の AI フレンドリーなメーラー）のリポジトリです。
作業を始める前に、必ず次の 3 つを読んで、リポジトリの方針とフェーズを把握してください。
- CLAUDE.md（作業ルールと現在のフェーズ）
- docs/DESIGN.md（アーキテクチャ・スキーマ・MCP ツール設計）
- docs/design/README.md と docs/design/tokens.css（UI デザインの正）

今回のタスクは **P2: メイン画面の UI 実装** です。バックエンド（IMAP 同期、MCP）はまだ無いので、モックデータで画面を完成させます。

環境は **Windows ネイティブ**（WSL ではない）です。シェルは PowerShell 前提でコマンドを書いてください。
最初に `rustup`（MSVC）、`cargo`、`node`、`pnpm`、Visual Studio Build Tools（C++ デスクトップ開発）、WebView2 の有無を確認し、
足りないものがあれば作業を始める前に何をインストールすべきか教えてください（勝手にインストーラを走らせない）。
`pnpm` が無ければ `corepack enable` で有効化して構いません。

Use the claude_design MCP (https://api.anthropic.com/v1/design/mcp, auth via /design-login) to import this project: <Claude Design のプロジェクト URL（handoff 文からコピー）>
Focus on these files (the whole project is readable):
- `Meowbox Main Dark.dc.html`
Also read these files the selection imports:
- `support.js`

Implement: `Meowbox Main Dark.dc.html` — as the main screen of the Meowbox desktop app, under the constraints below. docs/design/main-dark.png が完成イメージ、docs/design/main-dark.reference.html が寸法・余白の参照です。

## 1. 技術スタックと配置
- `apps/desktop/` に Tauri v2 + React 18 + TypeScript + Vite のアプリを新規作成する（`pnpm create tauri-app` 相当の構成）。パッケージマネージャは pnpm。
- ルートの Cargo.toml の workspace `members` に `apps/desktop/src-tauri` を追加し、`cargo build --workspace` が通るようにする。src-tauri は `mailcore` と `mailstore` に path 依存を張っておく（今回はまだ呼ばないが、P3 で invoke に繋ぐため）。
- スタイルは Tailwind v4。`docs/design/tokens.css` を `apps/desktop/src/styles/tokens.css` にコピーし、`@theme` で CSS 変数を Tailwind のカラー / フォント / 角丸に流し込む。
- **色・フォント・角丸はトークン経由のみ。** コンポーネントに HEX や rgba を直書きしない。デザインにあってトークンに無い値が必要なら、先に tokens.css に名前を付けて追加してから使う。
- フォントは `@fontsource/noto-sans-jp`（400/500/700）と `@fontsource/ibm-plex-mono`（400/500）をバンドルする。オフラインで動くアプリなので Google Fonts へのリンクは使わない。
- アイコンは `lucide-react`。状態管理は `zustand`（小さく）。それ以外の UI ライブラリは入れない。

## 2. コンポーネント分割
デザインの「左サイドバー / 中央スレッド一覧 / 右スレッド表示 / 右端パネル」をそのまま分割する。`apps/desktop/src/components/` 配下:
- `layout/AppShell` — 3 ペイン + 右パネルのグリッド。sidebar 220px 固定、list 390px 固定、thread が伸縮、panel 300px（トグル）
- `sidebar/Sidebar`, `sidebar/ProjectGroup`（案件名・未読バッジ・同期ドット・所属アドレス）, `sidebar/ViewItem`, `sidebar/SyncStatus`
- `list/ThreadList`, `list/ThreadRow`（要対応の左アクセントバー、未読ドット、差出人、件名、AI 要約 1 行（`--ai` 色）、日時、案件タグ、添付アイコン、タスク数バッジ、選択状態、ホバー時クイックアクション）
- `thread/ThreadView`, `thread/ThreadHeader`（件名・案件タグ・参加者・アーカイブ / フラグ / メニュー）
- `thread/AiSummaryCard`（「Claude による要約」ラベル、生成時刻、再生成ボタン。背景・枠は `--ai-bg` / `--ai-border`）
- `thread/ExtractedTasks` + `thread/TaskRow` — **確定（実線枠・チェックボックス・期日）** と **候補（点線枠・「候補・確度 62%」・確定 / 却下ボタン）** の 2 状態
- `thread/MessageItem`（アバター、差出人、日時、本文、「引用 N 行を表示」折りたたみ、添付チップ）
- `thread/ReplyBox`（プレースホルダ「山田さんへ返信…」、「✦ AI で下書き」ボタン（`--ai`）、ヒント「Tab で挿入・編集してから送信」、**「確認して送信」ボタン（`--accent`）**）
- `panel/DigestPanel`（「今日のダイジェスト」+ 日付、「Claude のまとめ」カード、案件ごとのタスク（確定 / 候補））
- `ui/Badge`, `ui/Kbd`, `ui/IconButton`, `ui/Checkbox`
デザインの `rightPanelOpen` prop は zustand の state にして、⌘\ でトグルできるようにする。

## 3. 型とモックデータ
- `apps/desktop/src/types.ts` に、`crates/mailcore/src/lib.rs` の `Account` / `AccountKind` / `MessageSummary` / `Message` / `Address` / `Task` / `TaskStatus` / `Draft` に対応する TypeScript 型を定義する。フィールド名・enum 値は Rust 側の serde 表現（snake_case）に合わせる。P3 で Tauri invoke の戻り値をそのまま流し込めるようにするため。
- UI 専用の派生型（スレッド一覧の 1 行、ダイジェスト項目など）は `types.ui.ts` に分け、コア型と混ぜない。
- `apps/desktop/src/mock/` にデザインと同じサンプルデータを型付きで置く:
  - 案件: 自社（me@my-company.example）、案件A（a-project@client-a.co.jp, a-project@gmail.com）、案件B（b@client-b.onmicrosoft.com）
  - スレッド: 「【至急】本番環境でエラー」佐藤 誠（要対応）、「Re: 見積の件」山田 太郎（選択中・4 通・添付 追加要件一覧.xlsx）、「9月定例のご案内」総務部 情報システム課、「Re: 商品画像の差し替えについて」鈴木 花子、「Weekly digest」GitLab、「8月分検収書のご送付」経理部 高橋
  - 「見積の件」の要約文、タスク「見積書を送る（9/5 確定）」「追加要件のヒアリング日程調整（候補・確度 62%）」、ダイジェストの内容もデザインの文言をそのまま使う
- データアクセスは `src/api/` に `listAccounts()` / `listThreads()` / `getThread()` / `getDigest()` のような関数として切り、今はモックを返す。P3 でこの層だけを `invoke()` に差し替える。

## 4. 振る舞い（この段階で入れるもの）
- 一覧のクリックでスレッド切替、選択行のハイライト
- キーボード: `j` / `k` で一覧移動、`e` アーカイブ（モック: 一覧から消える）、`r` で返信欄フォーカス、`⌘\` で右パネル、`⌘K` はコマンドパレットの空モーダル（中身は P5）
- 「AI で下書き」を押すと、返信欄に `--ai` 色の枠で下書きが挿入される（文面はモック固定）。挿入後にユーザーが 1 文字も編集せず「確認して送信」を押したら、確認ダイアログ「AI の下書きをそのまま送信しますか？」を出す。送信自体はモック（トーストを出して下書きを消す）
- タスクの「確定」「却下」で候補が確定 / 消滅する（state のみ）
- 引用の折りたたみ、添付チップのホバー
- ライトテーマは未デザインなので**今回は実装しない**。ただし全部トークン経由なので、後で `[data-theme="light"]` で値を差し替えれば切り替わる構造にしておく

## 5. 守ること（CLAUDE.md の再掲 + UI 固有）
- 人間側の操作（送信・選択・要対応・フラグ）は `--accent` 系、Claude 由来の要素（要約カード・AI 抽出タスク・AI 下書き・「Claude のまとめ」）は `--ai` 系。**絶対に混ぜない。** 一目で「誰が書いたか」分かることがこのアプリの安全設計の一部。
- 送信ボタンのラベルは常に「確認して送信」。「送信」だけにしない。
- 密度はデザイン通り（基本 12.5px / 行間 1.6、余白 6〜14px）。勝手に余白を広げたり角丸を大きくしたりしない。
- メールアドレス・数値・時刻・キーボードショートカットは `--font-mono`。
- 1440×900 でデザインとピクセル単位で近いことを最優先。ウィンドウを広げたときはスレッド表示だけが伸びる。狭めたときは右パネル → サイドバーの順で自動折りたたみ。
- 小さい PR 単位で進める。順番は (1) Tauri 雛形 + Tailwind + tokens (2) AppShell + Sidebar (3) ThreadList (4) ThreadView 一式 (5) DigestPanel (6) キーボード・振る舞い。各ステップで `pnpm build` を通す。

## 6. 完了条件
- `pnpm tauri dev` で起動し、docs/design/main-dark.png と同じ画面がモックデータで表示される
- `cargo build --workspace`、`pnpm build`、`pnpm tsc --noEmit` が通る
- 実際に 1440×900 でスクリーンショットを撮って `docs/design/impl-main-dark.png` に保存し、main-dark.png と見比べて差分（色・余白・欠けている要素）を箇条書きで報告する
- README.md の Quick start に `pnpm install` / `pnpm tauri dev` を追記する
- CLAUDE.md の「現在のフェーズと次の一手」で P2 にチェックを入れ、P3 の項目を「`src/api/` のモックを Tauri invoke → mailstore に差し替え」と具体化して書き足す

分からない点や、デザインと CLAUDE.md の方針が矛盾する点があれば、勝手に決めずに先に質問してください。

---- ここまで ----
