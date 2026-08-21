# 0008. VAD は whisper.cpp 内蔵 Silero を独立適用（whisper-rs の `state.full()` は内蔵VADをバイパス）

- ステータス: 採用（ADR-0007 末尾「VAD 必須」の実装。spec §2/§6 の「Silero VAD（ONNX, sherpa 同梱）」を訂正）
- 日付: 2026-06-24

## Context

whisper は無音区間で「ご視聴ありがとうございました」等を**ハルシネーション**する（ADR-0007）。Phase 1a の文字起こしには、これを抑える VAD（無音区間除去）が必要。

当初 spec は VAD を **Silero VAD（ONNX, sherpa-onnx 同梱）**で行う想定だった。しかし sherpa-onnx は **Phase 2 の話者分離**で導入予定（ADR-0004）であり Phase 1 にはまだ無い。一方 whisper.cpp は **Silero VAD（ggml）を内蔵**し、whisper-rs 0.16 がそれを公開している。

whisper.cpp の VAD には 2 つの使い方がある：

1. **組み込み**: `FullParams::enable_vad` + `set_vad_model_path`。`whisper_full` が内部で VAD を適用してから文字起こしする。
2. **スタンドアロン**: `WhisperVadContext` で発話区間を検出し、呼び出し側が音声を切り出す。

組み込み(1)が一見シンプルだが、**whisper-rs 経由ではバイパスされる**ことをソース確認で突き止めた：

- whisper-rs の文字起こしは `WhisperState::full()` 経由で、これは C 関数 **`whisper_full_with_state` を直接呼ぶ**（whisper-rs 0.16 `src/whisper_state/mod.rs`）。
- whisper.cpp 側では VAD フィルタは **`whisper_full` / `whisper_full_parallel` の中**（`whisper_full_with_state` を呼ぶ**前**）にしか無い。`whisper_full_with_state` 本体（`src/whisper.cpp` の 6792 行定義）は `params.vad` を参照しない。VAD フィルタは 7749 / 7777 行（`whisper_full` 側）。
- 結果、`enable_vad` を立てても **`state.full()` 経由では VAD が無視される**。

## Decision

- **スタンドアロン VAD を採用**する。`stt` モジュールで `WhisperVadContext`（Silero ggml）により発話区間を検出し、無音を除いた PCM を作って `state.full()` に渡す。
- VAD モデルは `ggml-org/whisper-vad` の `ggml-silero-v5.1.2.bin` を初回 DL（**best-effort**: 取得失敗時は VAD 無しで文字起こしを続行）。
- **タイムスタンプ再マッピング**を自前で持つ（`stt::filtered_ms_to_original`）。whisper が返す filtered-time のセグメント時刻を元音声の時刻へ戻す。区間境界では**開始を次区間・終了を前区間**へ寄せ、終了が無音ギャップを飛び越える誤りを防ぎ、変換後の単調非減少を保証する（unit test 済み）。
- **Phase 1 で sherpa-onnx を前倒し導入しない**（依存が重く Phase 2 まで不要）。ADR-0004（話者分離 = sherpa-onnx）は不変。

## Consequences

- ✅ Phase 1 で追加の重い依存（onnxruntime / sherpa）なしに VAD を実現。whisper と同じ ggml スタックなので ADR-0007 の衝突問題も無い。
- ✅ 無音ハルシネーションを抑制（実音声で検証）。
- ⚠️ タイムスタンプ再マッピングを自前で保守する。**既知の制限**（いずれも再マッピング自体は正しく、whisper のセグメント生成／パディングに起因）:
  - **無音をまたぐセグメント結合**: whisper が VAD 除去区間（無音）をまたいで連続発話を 1 セグメントにまとめると、そのセグメントは**開始が自身の発話より前（無音前の区間）**に置かれ、見かけの長さが無音分膨らむ。実音声検証で 1 例観測（`[38.5s-52.5s]`、テキストは無音後の発話だが開始が無音前）。シーク位置が約 12s 早まる実害が出たら、VAD 区間境界でセグメントを分割する（beta では未対応）。
  - **近接発話のパディング重複**: 間隔が `2×VAD_PAD_MS = 400ms` 未満の発話はパディング領域が重複し、再マッピングが非単調になり得る。実害が出たら詰める。
- ⚠️ Phase 2 で sherpa-onnx 導入時、diarization の発話区間と VAD を **1 パスに統合**できるか再検討する。その受け皿として `vad::Vad` / `vad::SpeechSpan` 抽象を残置している。

## Alternatives

- **組み込み VAD（`enable_vad`）**: 不可。上記のとおり whisper-rs の `state.full()` ではバイパスされる。`WhisperContext::full`（非 state API）なら効くが、whisper-rs はセグメント反復に state API を用いており、そちらへ寄せる利点はない。
- **sherpa-onnx VAD を前倒し**: 依存が重く Phase 2 まで不要。
