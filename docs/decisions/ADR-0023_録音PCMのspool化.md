# 0023. 録音 PCM の spool 化（キャプチャ中のディスク逐次書き出し）

- ステータス: 採用
- 日付: 2026-07-05
- 関連: [[ADR-0017_会議モードのシステム音声キャプチャ]]（デュアルトラック録音）/ [[ADR-0021_FFI例外シールドと重処理直列化]]（メモリ枯渇によるクラッシュの系譜）

## Context

mic / system 両キャプチャは PCM 全量を `Vec<f32>` に蓄積していた。48kHz f32 で
**2 時間 ≈ 1.4GB/トラック**（会議モードは 2 トラック）、さらに停止時の再生用結合ミックスが
数 GB 級の一時バッファを作る。ターゲットの 16GB 機では長時間会議がメモリ圧迫 →
swap フリーズ/クラッシュのリスクゾーンで、v0.3.0 の実機クラッシュの根本原因も
メモリ枯渇（bad_alloc、ADR-0021）だった。ffi_guard は例外を Err 化するが、
キャプチャ中の蓄積自体は防げない。

## Decision

**キャプチャ中に定期的に WAV へ追記し、メモリ上のバッファを有界にする。**

- `src-tauri/src/audio/spool.rs`
  - `SharedPcm`: base（絶対 index）付き共有バッファ。音声コールバックは `push` のみ
    （IO なし）。flush で先頭を捨てても読者が位置を見失わない。
  - `WavSpoolWriter`: hound の追記ライタ。**毎 flush でヘッダを現在長に更新**するため、
    SIGKILL/クラッシュでも直近 flush 時点まで有効な WAV が残る。
- キャプチャワーカー（mic.rs / system_audio.rs）: `stop_rx.recv()` を
  `recv_timeout(5s)` ループに変え、**末尾 30 秒を残して** spool へ append+flush。
  flush の IO エラー（disk full 等）はチャンク破棄で録音継続（部分保存 > 全損）し、
  `StopInfo.spool_error` に記録する。常駐メモリは **~13MB/トラックで一定**。
- spool パスは `recordings/.spool/<session-uuid>-{mic,system}.wav`。停止時に正式名
  （`<id>.wav` / `<id>-mic.wav` / `<id>-system.wav`）へ **rename** で確定
  （recordings/ 配下に置くのは**同一ボリューム保証**のため）。cancel は削除、
  クラッシュ残骸は起動時に `.spool/` ごと掃除（lib.rs setup）。
- ライブ文字起こし（live_stt）は `snapshot_from`（絶対 index）で追従。flush に
  追い越されたら（skipped）ローカルバッファを捨てて**末尾揃えで再同期**する
  （プレビューは使い捨てなので数秒の欠落は許容。KEEP_TAIL=30s は重処理中の長い
  whisper tick も概ね吸収する）。
- 再生用結合ミックスは `audio/mix.rs` の **ストリーミング実装**へ差し替え:
  per-track WAV をチャンク読み → mono 化 → `ChunkResampler`（既存
  `resample_linear_mono` と**ビット一致**する絶対 index 写像・チャンク境界 carry 付き）
  → 加算クランプ → 逐次書き込み。ピークメモリは数 MB。

## 検証

- `ChunkResampler` はランダム分割でバッチ版と完全一致、`write_mixed_wav` は
  旧パイプライン（to_playback_mono ×2 + mix_mono + write_wav）と**バイト一致**を
  テストで固定。
- `SharedPcm` の flush/snapshot 境界、`WavSpoolWriter` の分割 append =
  旧 write_wav バイト一致、flush 途中ファイルの可読性（クラッシュ耐性の近似）も
  単体テストで担保。

## 影響・制約

- WAV の data チャンクは u32 バイト上限（16bit mono 48k で約 12.4 時間、2ch で
  約 6.2 時間）。従来 write_wav と同一の制約で、超過は spool_error として現れる。
- 録音 state はプロセス内前提のまま（多重プロセス起動時は起動時掃除が他プロセスの
  spool を消し得る — 従来と同じ制約）。
- δ/ドリフト未補正の粗いミックスである点は従来どおり（ADR-0017。文字起こしは
  per-track の元 WAV で正確）。
