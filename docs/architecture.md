# mojiroku アーキテクチャ

> 関連: [要件定義書](./requirements.md) ・ [仕様書](./spec.md) ・ [ADR](./decisions/)

案B（Rust 単一ランタイム）。ML をすべて Rust から in-process で駆動し、Python・サイドカー・
localhost HTTP を持たない。最大リスクだった「PyInstaller で torch を凍結する」配布が消える。

## 全体図

```
┌──────────────────────────────────────────────────────────┐
│  Tauri v2 デスクトップアプリ (mojiroku)                     │
│  ┌───────────────┐   invoke / event   ┌─────────────────┐ │
│  │ Vite + React  │◄──────────────────►│ Rust コア        │ │
│  │ UI (静的出力)  │  (IPC, HTTP なし)   │ ・Tauri commands │ │
│  └───────────────┘                    │ ・音声取り込み    │ │
│                                       │ ・ML パイプライン │ │
│   全 ML を in-process / Rust で実行 ──►│  whisper-rs      │ │
│   (サイドカーなし・localhost なし)      │  sherpa-onnx-rs  │ │
│                                       │  llama-cpp-2     │ │
│                                       └────────┬─────────┘ │
└────────────────────────────────────────────────┼──────────┘
                                                 ▼ (BYOK 選択時のみ)
                              OpenAI・Anthropic API (任意, reqwest)
```

## データフロー（処理パイプライン）

```
音声取り込み(file/mic/live)
   → VAD (Silero/ONNX, 無音除去)
   → STT (whisper-rs, ggml/Metal)              … Segment[]{start,end,text}
   → [Phase 2] 話者分離 (sherpa-onnx, ONNX)      … Speaker turns
   → 話者マージ (Segment ↔ Speaker turn 整合)     … Segment{..,speaker_id}
   → 要約/議事録 (llama-cpp-2 既定 / BYOK)        … Summary + ActionItem[]
   → 永続化 (SQLite) + UI へ event で進捗/結果
```

- モデルは **load/unload** で切り替え、8GB Mac でも同時常駐を避ける。
- 進捗は Tauri `event`（`job://progress`）で UI にストリーム。

## コンポーネント責務

| コンポーネント | 責務 | 非責務 |
|---|---|---|
| `frontend` | 画面・状態・ユーザー操作 | ML・ファイル I/O（コア経由） |
| `src-tauri` | ウィンドウ/権限/音声取り込み/UI↔コア橋渡し/配布 | ML ロジック本体（コアに委譲） |
| `crates/mojiroku-core` | STT/話者分離/要約/パイプライン/永続化/モデル管理 | UI / Tauri 依存 |

`mojiroku-core` を UI 非依存に保つことで、単体テスト可能・将来 CLI 等へ再利用可能にする。

## 主要な決定（ADR へのリンク）

- [0002 Tauri v2](./decisions/ADR-0002_デスクトップ基盤にTauri-v2を採用.md) — デスクトップ基盤
- [0003 Rust 単一ランタイム](./decisions/ADR-0003_MLをRust単一ランタイムに集約.md) — Python/サイドカー全廃・案C 梯子
- [0004 sherpa-onnx 話者分離](./decisions/ADR-0004_話者分離はsherpa-onnxで実現.md) — pyannote seg-3.0 の ONNX 重みのみ
- [0005 whisper.cpp](./decisions/ADR-0005_STTエンジンにwhisper-cppを採用.md) — Metal 前提の修正・受容トレードオフ
- [0006 Vite フロント](./decisions/ADR-0006_フロントはViteでNextは不採用.md) — Next.js 不採用

## 未確定（次フェーズの宿題）

1. **既定 GGUF モデルの選定** — 日本語議事録品質の実機検証（spec §8）。
2. **話者分離のクラスタリングしきい値** — 話者数未知の会議での実用可否を決める（spec §9）。
3. **モデルの同梱 vs 初回 DL** — 総フットプリントと初回体験のバランス（spec §10/§14）。
4. **案B/案C の最終確定** — 日本語 DER スパイクの結果次第（[ADR-0004](./decisions/ADR-0004_話者分離はsherpa-onnxで実現.md)）。
