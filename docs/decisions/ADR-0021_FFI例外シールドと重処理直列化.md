# 0021. FFI 例外シールドと重い ML ジョブの直列化（クラッシュ/フリーズ対策）

- ステータス: 採用
- 日付: 2026-07-03
- 関連: [[ADR-0007_要約llamaを別バイナリsidecarに分離]]（プロセス分離の先行例）/ [[ADR-0009_話者分離スパイク結果]]（whisper × sherpa 同居）

## Context

v0.3.0 の実機（M3 / 16GB / macOS 26.5.1）で、会議モードやファイル文字起こし＋話者分離を
重ねて使うと**アプリがよく落ちる／フリーズする**報告。クラッシュレポート（docs/error.md ほか
DiagnosticReports 計 3 件）を解析した結果、**3 件すべて同一署名**だった:

```
Exception: EXC_CRASH (SIGABRT), abort() called
Triggered by Thread: tokio-rt-worker
（strip 済みバイナリを atos/逆アセンブルで解析 →
 "fatal runtime error: Rust cannot catch foreign exceptions, aborting"）
```

### 根本原因

静的リンクした C++ 推論系（whisper.cpp / sherpa-onnx / onnxruntime）が投げる **C++ 例外**
（高負荷時のため `std::bad_alloc` / `Ort::Exception` が濃厚）が、FFI 境界を unwind で素通りして
Rust 側 `spawn_blocking` の `catch_unwind` に到達。**Rust は外来（C++）例外を捕捉できず、
仕様としてプロセスごと abort** する。

- sherpa-onnx の C API は例外を catch しない（プレビルド静的 lib を DL してリンクするだけなので
  上流側の修正もできない）。onnxruntime はエラーを C++ 例外で通知する設計。
- フリーズは同じメモリ枯渇のもう一つの顔（スワップ・スラッシング）。16GB 機で
  whisper(Metal ~2GB) + onnxruntime セッション×3 + llama sidecar(4.4GB) + 会議停止時の
  数 GB 級 PCM 一時バッファが重なると容易に枯渇する。
- 別件: v0.2.0 で終了時に `ggml_metal_device_free` 内 `ggml_abort` の記録が 1 件
  （exit 時の static デストラクタ経路。v0.3.0 では未再現・保存済みデータに影響なし・保留）。

## Decision

**二段構え**: (1) 例外を境界で封じて「落ちない」ようにし、(2) そもそも枯渇させない。

### 1. FFI 例外シールド（`mojiroku-core::ffi_guard`）

小さな C++ シム（`src/ffi_guard.cc`、cc crate でビルド）を追加:

```
Rust guard() ─→ C++ mojiroku_cpp_guard (try/catch) ─→ extern "C-unwind" トランポリン ─→ クロージャ（whisper/sherpa FFI）
```

- C++ 例外（`std::exception` 派生）は C++ 側 try/catch で捕捉し、`CoreError::Native`
  （what() 文言＋「メモリ不足の可能性」）として Rust に返す → UI にはエラー表示、**アプリは生存**。
- **`catch (...)` は書かない**: Rust panic（外来例外として C++ を通過）を握り潰すと
  "Rust panics must be rethrown" で abort するため。`std::exception` 派生のみ捕捉し、
  Rust panic はシールドを素通し（導入前と同一挙動。単体テストで実証済み）。
- トランポリンは `extern "C-unwind"`（unwind の通過を合法化）。トランポリン内で
  `catch_unwind` してはいけない（外来例外が触れた瞬間 abort。それが元のバグ）。
- 適用箇所: `WhisperStt::load` / `WhisperStt::transcribe`（VAD 含む）/ `SherpaDiarizer::diarize`
  （consolidate の埋め込み抽出含む）。**whisper / sherpa を呼ぶ新経路は必ず guard を通すこと。**
- 限界: ggml の `GGML_ABORT`（abort() 直呼び）は例外ではないので防げない。それも防ぐには
  sidecar 化（プロセス分離・ADR-0007 の構図）が必要。今回の実クラッシュは全件例外経路
  だったため見送り（将来の選択肢として残す）。

### 2. 重い ML ジョブの直列化（`commands::HEAVY_ML_JOB`）

`tokio::sync::Semaphore(1)` で **STT / 話者分離 / ローカル要約 sidecar をアプリ全体で同時 1 本**に:

- 対象: `transcribe_file` / `stop_mic_recording` / `stop_meeting_recording`（結合ミックス含む）/
  `summarize`（ローカル sidecar 経路。クラウド BYOK は軽いので対象外）。
- 録音停止系は **WAV 保存後**に permit を待つ（順番待ちで録音データを失わない）。
- 待ちに入るとき `stage="queued"` を進捗イベントに流し、UI（ProcessingOverlay / TemplateModal）が
  「他の処理の完了を待っています…」を表示。
- ライブ文字起こし（会議プレビュー）は**ソフトに譲る**: 重いジョブ実行中は tick をスキップし、
  溜まりすぎたバッファは捨てて前進（プレビューは使い捨て・メモリ有界）。permit は取らない
  （会議全体をブロックしないため）。

### 3. メモリピーク削減（小粒）

- `diarize()` で consolidate 前に `OfflineSpeakerDiarization`（ONNX セッション 2 本）を drop
  （同時生存セッション 3 → 1）。

## Consequences

- 高負荷でネイティブ例外が出ても**クラッシュせず** UI にエラーが出る（再試行可能）。
- 重い処理は排他実行になり、並べた場合は順番待ちになる（16GB 機での安定性を優先。
  待ちは UI に明示）。
- mojiroku-core に C++ ソース 1 枚と build.rs が増える（cc crate）。ビルドへの影響は軽微。
- 検証: `ffi_guard` 単体テスト 3 本（C++ 例外→Err / Rust panic 素通し / 正常値）＋
  実音声で `transcribe_diarize_cli` E2E（シールド経由の whisper+sherpa 正常動作）。
