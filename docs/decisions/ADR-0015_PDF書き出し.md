# 0015. PDF 書き出し（ブラウザ印刷 = トップレベル window.print ＋ capability ＋ content-only CSS）

- ステータス: 採用
- 日付: 2026-06-27

## Context

Phase 6（連携/書き出し）のファイル書き出しスライス。会議（タイトル + 全要約 + 文字起こし）を **PDF**
として出す。固有の難所は **日本語（CJK）フォント**と **$0・完全ローカル**の両立。論点と選択肢:

1. **生成方式**: (A) ブラウザ印刷 `window.print()` →「PDF として保存」、(B) jsPDF + html2canvas（画像 PDF・
   直接保存）、(C) jsPDF/Rust に **CJK フォント埋め込み**（選択可テキスト・直接保存）。
2. **CJK の正確さ**: 埋め込み方式（C）は Noto Sans JP 同梱で十数 MB バンドル肥大（未署名 .dmg は現在 ~12MB）。
   webview レンダリング（A/B）は **OS のシステムフォント（Hiragino）**で描画＝画面と同一・**フォント同梱不要**。
3. **直接保存 vs 印刷パネル**: A は OS 印刷パネル経由（直接保存でない）。B/C は保存ダイアログへ直接。

裏取り済み（**並行調査 Workflow** で一次確認。[[verify-load-bearing-assumptions]]）:
- `window.print()` は **wry / macOS 11+ で動く（no-op ではない）**。WebKit が WKWebView の
  `printOperationWithPrintInfo:`（メインフレーム描画）を叩き OS 印刷パネル（「PDF として保存」含む）を開く。
  根拠: tauri#3066（クラッシュトレースが selector 呼び出しを証明）, wry#713（ダイアログは開くが余白差）,
  tauri `webview_window.rs` の `pub fn print()`。本リポは wry 0.55.1 / tauri 2.11.3・macOS 26 で該当世代。

実機で踏んだ**落とし穴（順に解消）**:
1. **隠し iframe の `iframe.contentWindow.print()`（サブフレーム印刷）は無反応** — WKWebView のネイティブ印刷は
   メインフレーム描画で、サブフレーム印刷経路に到達しない（＋ 0×0 + `document.write` でレイアウト未確定）。
   → **トップレベル `window.print()`** に寄せる。
2. **`webview.print not allowed`（capability ACL）** — Tauri v2 は `window.print()` を内部コマンド
   `webview.print` に配線し、権限 `core:webview:allow-print` を要求する。未付与だと**拒否され「何も起きない」**
   （初回症状の真因。DevTools コンソールの Unhandled Promise Rejection で確定）。
3. **`html,body,#root { height:100% }`（内側スクロール構成）で印刷が 1 ページにクリップ** — `@media print` で
   `height:auto; overflow:visible` に戻す（印刷既定値・ゼロリスク）。
4. **Tailwind preflight の `ul { list-style:none }` を継承し箇条書きの • が消える** — `#print-root ul` に
   明示 `list-style: disc`。

## Decision

**トップレベル `window.print()` ＋ capability `core:webview:allow-print` ＋ `@media print` content-only CSS**
（方式 A）で実装する。Rust もフォント同梱も新規依存も不要・$0・選択可テキスト・CJK は画面と同一。

- **権限**: `src-tauri/capabilities/default.json` に `core:webview:allow-print` を追加（焼き込みのため Rust 再ビルド要）。
- **content-only**: アプリ本体（`#root`）と全ポータル（オーバーレイ/ポップオーバー/トースト, いずれも body 直下）を
  `@media print { body > *:not(#mojiroku-print-root){display:none} }` で隠し、body 直下に差し込んだ**印刷専用ノード**
  `#mojiroku-print-root` だけを表示。画面では `@media screen` で印刷ノードを隠す。iframe を使わないので
  「印刷対象だけ」を CSS で確定。
- **コア純関数 `frontend/src/lib/print.ts`**:
  - `meetingPrintBody(detail)` … タイトル + メタ + 全要約 + 文字起こしの HTML フラグメント（印刷ノードにも
    standalone 文書にも使う単一の正）。`meetingPrintHtml` は doctype + style で包む薄いラッパ（将来の画像化再利用）。
  - `mdToHtml` … 見出し `#`（**二重マーク `## # 議題` の先頭ハッシュ群を剥がす**）、箇条書き → `<ul><li>`、
    **`| a | b |` 表 → `<table>`**、`---` 区切り線は除去、`**bold**` → `<strong>`。
    **HTML 制御文字 `&`/`<`/`>` をエスケープ**（未エスケープだと `Vec<String>` 等の角括弧表記でレイアウトが壊れる。
    Slack [[ADR-0014_Slack送信]] と同型の防御）。
  - `printMeetingPdf(detail)` … 印刷専用ノード + `@media print` style を注入 → 2×rAF 後にトップレベル `window.print()`。
    `afterprint` ／ フォールバック撤去・再入ガード・同期失敗時の巻き戻し。
- **既定ファイル名**: 「PDF として保存」の既定名は `document.title` 由来。**印刷中だけ会議名（`exportBaseName`）に
  差し替え**、後始末で復元（既定が `mojiroku` でなく `会議名_YYYY-MM-DD` になる）。
- **正直なラベル**: 直接保存でなく印刷パネル経由のため UI は「**PDF（印刷）**」と表示し、ヘルプに「印刷ダイアログから
  『PDF として保存』」と明記。

## Consequences / リスク

- ⚠️ **実 PDF でしか視覚品質は検証できない（load-bearing）**。`gs` でページ画像化して目視検証済み:
  タイトル/CJK（Hiragino）/見出し・太字・箇条書き(•)/**Q&A 表が罫線テーブル**/改ページ（見出し `break-after:avoid`・
  行 `break-inside:avoid`）/content-only（ダーク UI 混入なし）/既定ファイル名=会議名。実機で実 PDF 保存も確認。
- CJK は webview システムフォントで描画＝**フォント同梱ゼロ・バイナリ肥大ゼロ**。出力は画像化しないため**選択・検索可能**。
- 直接ファイル保存ではなく **OS 印刷パネル経由**（`.md/.txt/.srt` の保存ダイアログと挙動が異なる）。UI で開示。
- 配布版（署名 .app）でも JS 印刷経路と capability は同一だが、**配布ゲートで実機再確認**する（dev と挙動差の前例あり）。
- **却下した代替**:
  - (B) jsPDF + html2canvas = **画像 PDF**（非選択・サイズ大・改ページが手動スライス）。トリガー切替で再利用可だが不要に。
  - (C) CJK フォント埋め込み = **十数 MB のバンドル肥大**（$0・~12MB 制約に反する）・サブセット化が重い。
  - Rust `WebviewWindow::print()` = JS 経路が動いたため不要（動かない場合の保険。`@media print` CSS はそのまま流用可）。
- **申し送り（範囲外）**: `mdToHtml` は素朴な行ベース。Markdown リンク `[x](y)`・ネストリスト等の厳密変換は未対応。
  印刷パネルの「サイレント保存」「ダイアログ終了の確実な検知」は Tauri 公式 API 未提供（tauri#4917 OPEN）。

関連: [[ADR-0014_Slack送信]]（同じ esc/見出し剥がし方針）, [[ADR-0013_Notion書き出し]],
`docs/spec.md` §8, `frontend/src/lib/print.ts`。
