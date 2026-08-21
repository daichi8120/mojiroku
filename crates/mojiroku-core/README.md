# mojiroku-core — ML パイプライン（Rust）

UI / Tauri に依存しない**純粋な ML コア**。Cargo ワークスペースのライブラリクレートとして
単体テスト可能・将来 CLI 等への再利用可。STT・話者分離・要約をすべて Rust から in-process で駆動する
（**Python・サイドカー・localhost HTTP なし**）。

## ディレクトリ

```
src/
├── stt/            # whisper-rs（whisper.cpp / Core ML / Metal）。日本語 large-v3
├── diarization/    # sherpa-onnx-rs。pyannote segmentation-3.0 の ONNX 重みのみ + 話者埋め込み + クラスタリング（torch なし）
├── vad/            # Silero VAD（ONNX, sherpa 同梱）
├── summarize/      # 既定 = llama-cpp-2（GGUF）。BYOK アダプタ（OpenAI / Anthropic, reqwest）。共通トレイトで切替
├── models/         # モデル DL / キャッシュ / load-unload ライフサイクル（8GB Mac 対応）
├── store/          # 永続化（SQLite: rusqlite or sqlx）
└── schemas.rs      # データモデル（Recording / Transcript / Segment / Speaker / Summary / ActionItem / SummaryTemplate）
```

## 設計上の要点（spec の宿題）

- **話者分離は話者数未知** → しきい値ベースのクラスタリングが必須。このしきい値が日本語会議での実用可否を決める。詳細 [`../../docs/spec.md`](../../docs/spec.md) §9・[`../../docs/decisions/ADR-0004_話者分離はsherpa-onnxで実現.md`](../../docs/decisions/ADR-0004_話者分離はsherpa-onnxで実現.md)。
- **案C フォールバック**: sherpa-onnx の日本語品質が不足する場合のみ、torch pyannote を薄い Python サイドカーで Phase 2 に遅延起動。Phase 1 は Python ゼロを維持。
- **既定 GGUF の日本語議事録品質**が体験の下限。早期に実機検証する。

## 次フェーズで scaffold

`Cargo.toml`（依存: whisper-rs / sherpa-onnx / llama-cpp-2 / rusqlite / reqwest / serde 等）と各モジュールの実装。本ディレクトリは骨格のみ。
