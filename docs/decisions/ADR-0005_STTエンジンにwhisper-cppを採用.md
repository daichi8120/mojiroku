# 0005. STT エンジンに whisper.cpp（whisper-rs）を採用

- ステータス: 採用
- 日付: 2026-06-24

## Context

日本語の文字起こしには large-v3 級のモデルが必要で、速度が課題。当初は Python の
**faster-whisper** を想定していたが、その基盤 **CTranslate2 は Apple GPU / Metal に非対応**である：

- `device="mps"` を指定すると `ValueError: unsupported device mps` で弾かれる。
- CTranslate2 の Apple 向けバックエンドは Accelerate（**CPU 実行**）。
- つまり **Apple Silicon では faster-whisper は CPU 止まり**で、想定していた Metal 高速化は得られない。
  faster-whisper の GPU 高速化の利点は NVIDIA 上のもの。

これは「配布形態=デスクトップ（ネイティブ高速化で large-v3 実用）」という前提と矛盾する。
Metal で large-v3 を回す本命は **whisper.cpp**（Core ML / Metal 対応、M 系で large-v3 を実時間の約 10 倍で処理）。
加えて whisper.cpp は **ggml 系**で、同梱予定の llama.cpp と同じエコシステム
→ ランタイムを統一でき、torch / CTranslate2 を 1 つ減らせる（[ADR-0003](./ADR-0003_MLをRust単一ランタイムに集約.md)）。

## Decision

STT エンジンに **whisper.cpp**（Rust バインディング `whisper-rs`）を採用する。「Metal が効く」前提を
faster-whisper から whisper.cpp に正す。

## Consequences

- ✅ Apple Silicon で Core ML/Metal による実用速度。ggml で llama.cpp とランタイム統一。
- ✅ torch / CTranslate2 を排除し、パッケージングが軽くなる。
- ⚠️ **受容したトレードオフ**: word-timestamp / VAD / pyannote との話者整合は faster-whisper より手間。
  とくに Phase 2 で whisper.cpp のセグメント/word タイムスタンプと sherpa の話者ターンを突き合わせる
  **話者マージ**の実装コストが乗る（[spec](../spec.md) §6）。Phase 2 実装者はこの前提で着手すること。
