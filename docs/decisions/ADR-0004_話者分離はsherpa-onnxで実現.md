# 0004. 話者分離は sherpa-onnx（pyannote seg-3.0 の ONNX 重みのみ）で実現

- ステータス: 採用
- 日付: 2026-06-24

## Context

確定した製品決定は「**話者分離を本格対応する（誰が話したか）**」。素朴な実装は `pyannote.audio` だが、
これは **PyTorch 一式**を要求し、Python サイドカー＋PyInstaller 凍結という最大リスクを生む
（[ADR-0003](./ADR-0003_MLをRust単一ランタイムに集約.md)）。

一方、話者分離を **torch なし**で回す道がある：**sherpa-onnx** は pyannote の
segmentation-3.0 を ONNX 化したモデルに話者埋め込み＋クラスタリングを組み合わせ、
**onnxruntime だけで（Python 不要・オフライン）**話者分離を行い、Rust API も提供する。

## Decision

話者分離を **sherpa-onnx** で実装する。**pyannote segmentation-3.0 の ONNX 重みのみ**を採用する。

> ⚠️ 重要な区別: これは **full `pyannote.audio` パイプラインではない**。pyannote の
> segmentation モデル（重み）を使い、埋め込み/クラスタリングは別スタックで構成する。
> 「pyannote 本格対応」という決定を、torch を捨てつつ実質的に満たす再解釈である。

### 設計上の肝（実機チューニング項目）

- 会議は**話者数が未知**。sherpa-onnx のオフライン diarization は「話者数既知」または
  「**クラスタリングしきい値**」を要求する → しきい値ベースを採用する。
- **このしきい値が日本語会議での実用可否を決める真の指標**（生 DER の数字より重要）。

### フォールバック（案C）

しきい値調整でも日本語品質が不足する場合に限り、torch pyannote(full pyannote.audio) を
薄い Python サイドカーで Phase 2 のみ遅延起動する。diarization トレイトの実装差し替えで切替える。

## Consequences

- ✅ torch を排除しつつ pyannote の segmentation 品質（重み）を活かせる。Rust 単一ランタイムと両立。
- ✅ Apple Silicon では CoreML で高速。
- ⚠️ 埋め込み/クラスタリングが pyannote.audio と異なるため、品質は同一保証ではない → 早期スパイクで検証。
- ⚠️ しきい値チューニングが品質の主たる調整点。
- 将来、Rust ネイティブ diarization（speakrs 等、ort 2.0 安定後）への置換余地。
