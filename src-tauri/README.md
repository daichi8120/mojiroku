# src-tauri — mojiroku デスクトップシェル

**Tauri v2** アプリ（Rust バイナリクレート）。役割は**薄いシェル**に限定し、ML の重い処理は
[`../crates/mojiroku-core`](../crates/mojiroku-core) に委譲する。

- ウィンドウ・権限（capabilities）・配布パッケージング
- 音声取り込み（ファイル / マイク / [Phase 4] システム音声）
- UI ↔ コアの橋渡し（Tauri コマンドでジョブ起動、`event` で進捗を返す）

旧案（Python サイドカー + localhost HTTP）は廃止。理由は
[`../docs/decisions/ADR-0003_MLをRust単一ランタイムに集約.md`](../docs/decisions/ADR-0003_MLをRust単一ランタイムに集約.md) を参照。

## ディレクトリ

```
src/
├── main.rs / lib.rs   # エントリ・アプリ初期化
├── commands/          # Tauri コマンド（#[tauri::command]）。start_job / cancel_job / get_history / export / set_provider 等
└── audio/             # 音声取り込み（OS 別）。Phase 4 で macOS ScreenCaptureKit 等のシステム音声
capabilities/          # Tauri v2 権限定義
icons/                 # アプリアイコン
```

## 次フェーズで scaffold

`cargo` ワークスペースのメンバーとして `tauri init` 相当を整備（`Cargo.toml` / `tauri.conf.json` / `build.rs`）。本ディレクトリは骨格のみ。
