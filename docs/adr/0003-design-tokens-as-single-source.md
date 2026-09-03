# 3. 色・字・角丸の出所をデザイントークンだけにする

- 状態: 採用
- 日付: 2026-09-03

## 背景

UI は Claude Design で作った画面を実装に落としている。デザインからは
`docs/design/tokens.css`（色・文字サイズ・角丸・レイアウト幅）を抜き出してある。

実装のたびに HEX を直書きすると、デザインを更新したときに追随できない。
ライトテーマも未デザインのまま構造だけ用意しておきたい。

## 決定

- `docs/design/tokens.css` を唯一の出所とし、`apps/desktop/src/styles/tokens.css` は
  その同期コピーとする
- Tailwind v4 の `@theme inline` でトークンをユーティリティに流す。生成される CSS は
  `var(--accent)` のような**トークンを直接参照する**ので、`[data-theme]` を
  差し替えるだけで実行時に色が入れ替わる
- TSX / TS 内の HEX・`rgba()` 直書きは eslint（`no-restricted-syntax`）で禁止する
- 足りない値は、使う前に `tokens.css` に名前を付けてから使う

## 理由

- 「どこかに 1 箇所だけ違う灰色がある」状態を機械が防ぐ
- ライトテーマを後から足すとき、変更点が `tokens.css` の値だけに閉じる
- デザイン更新の差分が読める（トークン名は変わらず値だけ動く）

## 結果

- `@theme` のキー名には制約がある。`--color-base` と `--text-base` を両方定義すると
  Tailwind は `text-base` を色として解決してしまうため、面の色は `surface` と呼ぶ
- 同様に `--text-<key>--line-height` を省くと Tailwind 既定の行間が効いてしまうので、
  文字サイズを足したら行間も必ず一緒に指定する
- フォントは `@fontsource` でバンドルする。Google Fonts へのリンクは張らない
  （オフラインで動くこと自体が要件のため）
