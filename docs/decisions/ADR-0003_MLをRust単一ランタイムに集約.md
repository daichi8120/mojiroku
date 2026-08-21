# 0003. ML を Rust 単一ランタイムに集約（Python サイドカーを廃止）

- ステータス: 採用（案B）／**一部 [ADR-0007](./ADR-0007_要約llamaを別バイナリsidecarに分離.md) で訂正**
- 日付: 2026-06-24

> ⚠️ 訂正: 「whisper.cpp と llama.cpp を**同一バイナリに in-process 同居**」という部分は誤りだった
> （ggml シンボル衝突で whisper が壊れる）。ローカル要約 llama.cpp は別バイナリ sidecar に分離する。
> 「Python 廃止・全 Rust・ローカル・$0」という本 ADR の幹は維持。詳細は [ADR-0007](./ADR-0007_要約llamaを別バイナリsidecarに分離.md)。

## Context

当初案は **3 ランタイム**（TS フロント / Python サイドカー / Rust シェル）で、ML（STT=whisper、
要約=LLM、話者分離=pyannote）を Python の FastAPI サイドカーに置き、Tauri から spawn して
localhost HTTP で通信する構成だった。この案では配布時に **PyInstaller で PyTorch を凍結**する必要があり、
多 GB・フック破損が頻発する「パッケージング地獄」を計画自身が**最大の脅威**と名指ししていた。

検討の結果、Python を強制している唯一の住人は **pyannote(torch)** だけだと判明した：

- STT = whisper.cpp、要約 = llama.cpp はどちらも C++/ggml で **Rust バインディング**がある
  （`whisper-rs` / `llama-cpp-2`）。
- 話者分離も **sherpa-onnx** で torch なしに実行できる（[ADR-0004](./ADR-0004_話者分離はsherpa-onnxで実現.md)）。

## Decision

ML をすべて **Rust ネイティブ（in-process）** で駆動する **単一ランタイム構成（案B）** を採用する。
**Python・FastAPI・サイドカー・localhost HTTP・PyInstaller・torch を全廃**する。
UI ↔ コアは Tauri の `invoke`/`event` で接続する。

### フォールバックの梯子

ユーザー方針により DER 検証スパイクを待たず案B にコミット。ただし日本語話者分離が不足した場合のみ：

- **案C（保険）**: torch pyannote(full pyannote.audio) のためだけに**薄い Python サイドカーを Phase 2 で遅延起動**。
  whisper/llama は Rust ネイティブのまま、Phase 1 は Python ゼロを維持。diarization トレイトの実装差し替えで吸収。
- **案A（廃止）**: 全 Python サイドカー。whisper/llama を Python に置く必然性がもう無いため不採用。

## Consequences

- ✅ パッケージングの最大リスクがほぼ解消（単一 Rust バイナリ＋モデルファイル）。残るはモデル DL/同梱と署名のみ。
- ✅ サイドカー spawn/監視・localhost 認証という壊れやすい配管と攻撃面が消える。起動も速い。
- ⚠️ ML を含む全バックエンドが Rust に（dev-hub 初 Rust、習熟ランプは実コスト）。
- ⚠️ ML の機動力（モデル差し替え/実験）は Python エコシステムより硬い。既定は安定モデルに固定。
- ⚠️ 日本語 diarization の品質は未検証 → 案C を保険として保持。
