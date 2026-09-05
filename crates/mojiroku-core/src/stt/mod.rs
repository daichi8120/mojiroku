//! STT（音声→文字起こし）。whisper.cpp / whisper-rs（Core ML/Metal）。
//! 受容したトレードオフは `docs/05_decisions/ADR-0005_STTエンジンにwhisper-cppを採用.md` を参照。

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext,
    WhisperVadContextParams, WhisperVadParams,
};

use crate::error::{CoreError, Result};
use crate::schemas::{Segment, Transcript};

const SAMPLE_RATE_F: f32 = 16_000.0;

fn configure_decoder<'a, 'b>(params: &mut FullParams<'a, 'b>, language: Option<&'a str>) {
    params.set_language(language);
    // no_context clears history only when full() starts. The bundled whisper.cpp still
    // feeds decoded text into subsequent audio windows, allowing a mistaken phrase to
    // reinforce itself for the rest of a recording. Disable that rolling prompt too.
    // This keeps every window conditioned on its audio (ADR-0032).
    params.set_n_max_text_ctx(0);
}

/// 文字起こしエンジンの抽象。
pub trait SttEngine {
    /// 16kHz mono f32 PCM を文字起こしする。`language=None` で自動判定。
    fn transcribe(&self, pcm16k_mono: &[f32], language: Option<&str>) -> Result<Transcript>;
}

/// whisper.cpp による STT。モデルを 1 度ロードして使い回す。
pub struct WhisperStt {
    ctx: WhisperContext,
    /// VAD モデル（Silero, ggml）。Some なら無音区間をスキップしハルシネーションを抑制。
    vad_model_path: Option<PathBuf>,
}

impl WhisperStt {
    /// モデルをロード（GPU/Metal 有効）。`vad_model_path=Some` で VAD を有効化。
    pub fn load<P: AsRef<Path>>(model_path: P, vad_model_path: Option<PathBuf>) -> Result<Self> {
        // whisper.cpp の既定ログコールバックは逐トークンの大量ログを stderr に吐く
        // （54分会議で 45k 行超／全ログの 94%）。本クレートは whisper-rs を
        // log/tracing feature 無効でリンクしているため、install_logging_hooks() は
        // これらを Rust 側トランポリンに差し替え＝実質破棄する（出力先なし）。
        // tauri dev では子プロセスの stdout が node 経由でターミナルへ中継され、
        // この洪水が実行を体感的に停滞させる要因になるため無効化する。
        // 冪等（内部 Once で一度だけ作用）なので load 毎に呼んでよい。
        whisper_rs::install_logging_hooks();

        let mut cparams = WhisperContextParameters::default();
        cparams.use_gpu(true);
        let path = model_path.as_ref().to_string_lossy();
        // FFI 例外シールド: whisper.cpp（C++）のロード中の例外（bad_alloc 等）を Err に変換
        // し、プロセス abort を防ぐ（ffi_guard 参照）。
        let ctx = crate::ffi_guard::guard("whisper モデルのロード", || {
            WhisperContext::new_with_params(path.as_ref(), cparams)
        })?
        .map_err(|e| CoreError::Model(format!("whisper load: {e:?}")))?;
        Ok(Self {
            ctx,
            vad_model_path,
        })
    }
}

impl SttEngine for WhisperStt {
    fn transcribe(&self, pcm16k_mono: &[f32], language: Option<&str>) -> Result<Transcript> {
        // FFI 例外シールド: whisper.cpp（C++）の推論中の例外（メモリ枯渇の bad_alloc 等）を
        // Err に変換。シールド無しだと例外が tokio の catch_unwind に達してプロセスごと
        // abort する（docs/error.md の実クラッシュ）。
        crate::ffi_guard::guard("文字起こし (whisper)", || {
            self.transcribe_inner(pcm16k_mono, language, None)
        })?
    }
}

/// whisper の progress コールバックへ渡す借用コンテキスト。`full()` 実行中だけ有効な
/// スタックローカルを指す（借用なので `'static` 不要＝safe 版 set_progress_callback_safe が
/// 要求する 'static を回避するために unsafe 版を使う理由）。`last` は整数%が増えた時だけ
/// 発火させるスロットル（whisper は同一%を何度も呼ぶ・イベントバス洪水を避ける）。
struct ProgressCtx<'a> {
    cb: &'a dyn Fn(i32),
    last: std::cell::Cell<i32>,
}

/// whisper.cpp（C++）から呼ばれる progress コールバックのトランポリン（0-100%）。
///
/// ⚠️ 同一スレッド前提: whisper は **`full()` と同じスレッド**でチャンク境界ごとにこれを呼ぶ
/// （ggml ワーカースレッドではない）。だから `user_data` が指す `&ProgressCtx`（!Send・借用+Cell）
/// を触って安全。将来 whisper-rs 更新でこの前提が崩れると静かに UB になる（コンパイルは通る）。
///
/// ⚠️ 非パニック必須: plain `extern "C"`（`C-unwind` ではない）なので、Rust の unwind が C++
/// フレームへ抜けるとプロセス abort する（ffi_guard は C++ 例外用で Rust panic は救えない・ADR-0021）。
/// よって本体は emit のみ・添字/unwrap を持たないコールバック（`ProgressCtx::cb`）だけを通すこと。
unsafe extern "C" fn whisper_progress_trampoline(
    _ctx: *mut whisper_rs::WhisperSysContext,
    _state: *mut whisper_rs::WhisperSysState,
    progress: std::os::raw::c_int,
    user_data: *mut std::os::raw::c_void,
) {
    if user_data.is_null() {
        return;
    }
    let ctx = &*(user_data as *const ProgressCtx);
    if progress > ctx.last.get() {
        ctx.last.set(progress);
        (ctx.cb)(progress);
    }
}

impl WhisperStt {
    /// 進捗コールバック付き文字起こし（whisper 0-100% を `on_pct` へ）。FFI 例外シールドは
    /// [`SttEngine::transcribe`] と同じ。`on_pct=None` なら素の transcribe と等価。
    pub fn transcribe_with_progress(
        &self,
        pcm16k_mono: &[f32],
        language: Option<&str>,
        on_pct: Option<&dyn Fn(i32)>,
    ) -> Result<Transcript> {
        crate::ffi_guard::guard("文字起こし (whisper)", || {
            self.transcribe_inner(pcm16k_mono, language, on_pct)
        })?
    }
}

impl WhisperStt {
    fn transcribe_inner(
        &self,
        pcm16k_mono: &[f32],
        language: Option<&str>,
        on_pct: Option<&dyn Fn(i32)>,
    ) -> Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| CoreError::Model(format!("create_state: {e:?}")))?;

        // VAD（無音ハルシネーション対策）。whisper-rs の state.full() は内蔵VAD(whisper_full)を
        // バイパスするため、明示的に WhisperVadContext で無音を除去してから渡す。
        // 失敗時は元の PCM をそのまま使う（best-effort）。
        //
        // When the VAD runs fine but finds no speech at all, the answer is an empty transcript.
        // Handing the raw (silent) PCM to whisper instead is exactly the case the VAD exists to
        // prevent: 60 s of digital silence came back as two hallucinated segments (ADR-0031).
        // One diagnostic line per transcription so a user running a dev build can report what the
        // VAD kept (same pattern as the meeting track offset line). Live transcription calls this
        // every 3.5 s with a tail of at most 14 s; printing there would flood the terminal that
        // `tauri dev` relays, so only whole-recording inputs (>= 30 s) print.
        let diag = pcm_secs(pcm16k_mono) >= 30.0;
        let (pcm, time_map): (Cow<[f32]>, Option<Vec<TimeSpan>>) = match &self.vad_model_path {
            Some(vad) => match vad_filter(&vad.to_string_lossy(), pcm16k_mono) {
                Ok((filtered, _)) if filtered.is_empty() => {
                    if diag {
                        eprintln!(
                            "stt vad: no speech found in {:.1}s, skipping whisper",
                            pcm_secs(pcm16k_mono)
                        );
                    }
                    return Ok(Transcript {
                        language: language.map(|s| s.to_string()),
                        segments: Vec::new(),
                    });
                }
                Ok((filtered, map)) => {
                    if diag {
                        // kept = padded spans (VAD_PAD_MS on each side), so it runs above the raw
                        // Silero coverage; whisper input adds VAD_GAP_MS between spans on top.
                        let kept_ms: u64 = map.iter().map(|s| s.dur_ms).sum();
                        eprintln!(
                            "stt vad: {} spans, kept {:.0}% of {:.1}s incl. padding, whisper input {:.1}s",
                            map.len(),
                            kept_ms as f32 / 10.0 / pcm_secs(pcm16k_mono),
                            pcm_secs(pcm16k_mono),
                            pcm_secs(&filtered),
                        );
                    }
                    (Cow::Owned(filtered), Some(map))
                }
                Err(_) => (Cow::Borrowed(pcm16k_mono), None),
            },
            None => (Cow::Borrowed(pcm16k_mono), None),
        };

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // FullParams defaults to English. Calling set_language(None) is therefore required for
        // Whisper's language auto-detection; merely omitting this call silently forces English.
        configure_decoder(&mut params, language);
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // whisper 0-100% を on_pct へ流す。set_progress_callback_safe は `'static` を要求し
        // 借用 on_pct を通せないため、unsafe 版で **full() 実行中だけ有効な**借用コンテキストを
        // user_data に渡す（詳細は whisper_progress_trampoline の注記）。
        let progress_ctx = on_pct.map(|cb| ProgressCtx {
            cb,
            last: std::cell::Cell::new(-1),
        });
        if let Some(ctx) = &progress_ctx {
            // SAFETY: ctx は本関数のスタックに生き、下の full() は同期実行で、コールバックは
            // full() の内側からのみ・同一スレッドで呼ばれる。full() 完了後に user_data は使われない。
            unsafe {
                params.set_progress_callback(Some(whisper_progress_trampoline));
                params.set_progress_callback_user_data(
                    ctx as *const ProgressCtx as *mut std::os::raw::c_void,
                );
            }
        }

        // progress_ctx は named local として関数末尾まで生存する（full() 中の user_data 参照より長命）。
        state
            .full(params, &pcm)
            .map_err(|e| CoreError::Model(format!("full: {e:?}")))?;

        let mut segments = Vec::new();
        for seg in state.as_iter() {
            let text = seg
                .to_str_lossy()
                .map(|c| c.into_owned())
                .unwrap_or_default();
            // whisper のタイムスタンプは centiseconds（10ms 単位）→ ms
            let mut start_ms = seg.start_timestamp().max(0) as u64 * 10;
            let mut end_ms = seg.end_timestamp().max(0) as u64 * 10;
            // VAD でフィルタした場合は filtered-time → original-time に戻す。
            // 区間境界では開始は次区間へ、終了は前区間へ寄せ、無音ギャップの飛び越えを防ぐ。
            if let Some(map) = &time_map {
                (start_ms, end_ms) = remap_segment(map, start_ms, end_ms);
            }
            segments.push(Segment {
                // idx は保存時に insert_segments が enumerate で採番し直す（schemas.rs 参照）。
                idx: 0,
                start_ms,
                end_ms,
                text: text.trim().to_string(),
                speaker_id: None,
            });
        }

        Ok(Transcript {
            language: language.map(|s| s.to_string()),
            segments,
        })
    }
}

/// フィルタ後 PCM の区間と元 PCM の対応（時刻マップの 1 要素）。
struct TimeSpan {
    /// フィルタ後 PCM 上の開始時刻(ms)
    filtered_start_ms: u64,
    /// 元 PCM 上の開始時刻(ms)
    orig_start_ms: u64,
    /// 区間長(ms)
    dur_ms: u64,
}

const VAD_PAD_MS: u64 = 200;

/// VAD 区間（ms, 元時刻）へ前後パディングを付け、切り出すサンプル範囲へ変換する。
/// 隣接区間の間隔が 2×VAD_PAD_MS 未満だとパディング同士が重なるため、開始を前区間の
/// 末尾でクランプして**同じ音声を二重に切り出さない**（重複すると境界の語が二重に
/// 転写され得る）。返す範囲は互いに素で昇順。
fn padded_sample_ranges(segs_ms: &[(u64, u64)], total: usize) -> Vec<(usize, usize)> {
    let ms_to_sample = |ms: u64| ((ms as f32 / 1000.0) * SAMPLE_RATE_F) as usize;
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut prev_end = 0usize;
    for &(o0_ms, o1_ms) in segs_ms {
        let i0 = ms_to_sample(o0_ms.saturating_sub(VAD_PAD_MS)).max(prev_end);
        let i1 = ms_to_sample(o1_ms + VAD_PAD_MS).min(total);
        if i0 >= i1 {
            continue;
        }
        out.push((i0, i1));
        prev_end = i1;
    }
    out
}

/// Silero VAD で発話区間だけを抜き出した PCM と、filtered→original の時刻マップを返す。
fn vad_filter(model_path: &str, pcm: &[f32]) -> Result<(Vec<f32>, Vec<TimeSpan>)> {
    let mut vctx = WhisperVadContext::new(model_path, WhisperVadContextParams::new())
        .map_err(|e| CoreError::Model(format!("vad ctx: {e:?}")))?;
    let segs = vctx
        .segments_from_samples(WhisperVadParams::new(), pcm)
        .map_err(|e| CoreError::Model(format!("vad segments: {e:?}")))?;

    // centiseconds(10ms) → ms。
    let segs_ms: Vec<(u64, u64)> = segs
        .into_iter()
        .map(|seg| {
            (
                (seg.start.max(0.0) * 10.0) as u64,
                (seg.end.max(0.0) * 10.0) as u64,
            )
        })
        .collect();

    let ranges = padded_sample_ranges(&segs_ms, pcm.len());
    Ok(concat_ranges(pcm, &ranges))
}

/// Silence inserted between two speech ranges that were not adjacent in the original audio.
///
/// Gluing the ranges back to back hands whisper one continuous stream with no pauses, and it
/// then merges many short utterances into one long segment and drops the short replies
/// ("はい", "なるほど", "OKです") in between. A 1 s pause restores the utterance boundaries:
/// on a 257 s two-track meeting the segment count went from 26 to 39 (mic) and 35 to 71
/// (system) with the same content and ~20% more whisper wall time (ADR-0031).
const VAD_GAP_MS: u64 = 1000;

/// Concatenate the sample ranges into the PCM whisper will see, with [`VAD_GAP_MS`] of silence
/// between ranges that are not adjacent in the original, and build the filtered→original map.
/// Gap regions are not covered by the map; [`filtered_ms_to_original`] snaps times inside a gap
/// to the neighbouring range.
fn concat_ranges(pcm: &[f32], ranges: &[(usize, usize)]) -> (Vec<f32>, Vec<TimeSpan>) {
    let sample_to_ms = |s: usize| ((s as f32 / SAMPLE_RATE_F) * 1000.0) as u64;
    let gap_samples = ((VAD_GAP_MS as f32 / 1000.0) * SAMPLE_RATE_F) as usize;
    // filtered は各区間の連結長 + ギャップ、map は区間数だけ伸びる。事前予約で倍化 realloc の
    // ピークを避ける（発話支配的な長尺録音では filtered が原 PCM の大半に達しうる＝ADR-0021 の
    // 16GB 機メモリ枯渇面）。ranges は互いに素で各 i1 が pcm.len() で clamp 済＝合計は
    // pcm.len() 以下の信頼できる長さ。
    let total: usize = ranges.iter().map(|&(i0, i1)| i1 - i0).sum();
    let mut filtered: Vec<f32> =
        Vec::with_capacity(total + gap_samples * ranges.len().saturating_sub(1));
    let mut map: Vec<TimeSpan> = Vec::with_capacity(ranges.len());
    let mut prev_end: Option<usize> = None;
    for &(i0, i1) in ranges {
        if matches!(prev_end, Some(pe) if i0 > pe) {
            filtered.resize(filtered.len() + gap_samples, 0.0);
        }
        map.push(TimeSpan {
            filtered_start_ms: sample_to_ms(filtered.len()),
            orig_start_ms: sample_to_ms(i0),
            dur_ms: sample_to_ms(i1 - i0),
        });
        filtered.extend_from_slice(&pcm[i0..i1]);
        prev_end = Some(i1);
    }
    (filtered, map)
}

fn pcm_secs(pcm: &[f32]) -> f32 {
    pcm.len() as f32 / SAMPLE_RATE_F
}

/// フィルタ後の時刻(ms)を元 PCM の時刻(ms)に変換する。
///
/// span は filtered / original とも時刻昇順。filtered 上では連続して並ぶか、
/// [`VAD_GAP_MS`] の無音ギャップを挟む。span の境界（連続点）に時刻が一致したとき、
/// セグメント開始(`at_end=false`)は次区間の先頭へ、終了(`at_end=true`)は前区間の末尾へ
/// 割り当てる。ギャップの内側に落ちた時刻も同じ規則で、開始は次区間の先頭へ、終了は
/// 前区間の末尾へ寄せる。これにより終了時刻が無音ギャップをまたいで次区間へ飛ぶ誤りを
/// 防ぎ、変換後の時刻が単調非減少になることを保証する。
/// filtered 長を超える時刻は最終区間の末尾へクランプする。
fn filtered_ms_to_original(map: &[TimeSpan], t_ms: u64, at_end: bool) -> u64 {
    let Some(first) = map.first() else {
        return t_ms;
    };
    let mut chosen = first;
    let mut next: Option<&TimeSpan> = None;
    for span in map {
        // 終了は境界で前 span に留まり(`>`)、開始は次 span へ進む(`>=`)。
        let past = if at_end {
            t_ms > span.filtered_start_ms
        } else {
            t_ms >= span.filtered_start_ms
        };
        if past {
            chosen = span;
        } else {
            next = Some(span);
            break;
        }
    }
    // A start that falls inside an inserted silence gap belongs to the next span. The gap's first
    // sample (== the previous span's end) counts as inside: whisper and the VAD both work in
    // 10 ms units, so an exact hit is realistic, and a start left at the previous span's end
    // would place the subtitle before the removed silence.
    if !at_end && t_ms >= chosen.filtered_start_ms + chosen.dur_ms {
        if let Some(n) = next {
            return n.orig_start_ms;
        }
    }
    // 区間内オフセットは区間長でクランプ（filtered 長超過・ギャップ内の終了もここで吸収）。
    let offset = (t_ms - chosen.filtered_start_ms).min(chosen.dur_ms);
    chosen.orig_start_ms + offset
}

/// Map one whisper segment's `[start, end]` (filtered ms) to original time, keeping
/// `start <= end`. A segment that lies entirely inside an inserted silence gap would otherwise
/// get its start snapped forward and its end snapped backward, and the inverted interval would
/// reach the database and the SRT export. Such a segment is re-anchored at the next span's
/// start and keeps whisper's own duration, clamped to that span, so it still has a nonzero
/// interval for SRT cues and for speaker assignment by overlap (a zero-length whisper segment
/// stays zero-length, as it does without a gap).
fn remap_segment(map: &[TimeSpan], start_ms: u64, end_ms: u64) -> (u64, u64) {
    let s = filtered_ms_to_original(map, start_ms, false);
    let e = filtered_ms_to_original(map, end_ms, true);
    if e >= s {
        return (s, e);
    }
    let dur = end_ms.saturating_sub(start_ms);
    match map.iter().find(|sp| sp.filtered_start_ms > start_ms) {
        Some(next) => (
            next.orig_start_ms,
            next.orig_start_ms + dur.min(next.dur_ms),
        ),
        None => (s, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_disables_rolling_text_history() {
        // no_context alone only clears history at the start of full(), not between its
        // audio windows. Pin the separate history budget against dependency defaults.
        for language in [None, Some("ja"), Some("en")] {
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            configure_decoder(&mut params, language);
            let debug = format!("{params:?}");
            assert!(debug.contains("n_max_text_ctx: 0,"), "{debug}");
        }
    }

    #[test]
    fn auto_language_clears_whisper_english_default() {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        configure_decoder(&mut params, None);

        let debug = format!("{params:?}");
        assert!(debug.contains("language: 0x0"), "{debug}");
    }

    #[test]
    fn explicit_language_keeps_a_non_null_language_pointer() {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        configure_decoder(&mut params, Some("ja"));

        let debug = format!("{params:?}");
        assert!(!debug.contains("language: 0x0"), "{debug}");
    }

    /// filtered 0-2000ms→orig 1000-3000ms、filtered 2000-3500ms→orig 8000-9500ms。
    /// 元 time の無音 3000-8000ms を VAD が除去した想定（filtered 上では連続）。
    fn sample_map() -> Vec<TimeSpan> {
        vec![
            TimeSpan {
                filtered_start_ms: 0,
                orig_start_ms: 1000,
                dur_ms: 2000,
            },
            TimeSpan {
                filtered_start_ms: 2000,
                orig_start_ms: 8000,
                dur_ms: 1500,
            },
        ]
    }

    /// 16kHz: 1ms = 16 サンプル。
    const SPMS: usize = 16;

    #[test]
    fn padded_ranges_disjoint_segments_get_full_padding() {
        // 十分離れた 2 区間（1000-2000ms, 5000-6000ms）は前後 200ms パディング付きで独立。
        let r = padded_sample_ranges(&[(1000, 2000), (5000, 6000)], 10_000 * SPMS);
        assert_eq!(
            r,
            vec![(800 * SPMS, 2200 * SPMS), (4800 * SPMS, 6200 * SPMS),]
        );
    }

    #[test]
    fn padded_ranges_do_not_duplicate_overlapping_padding() {
        // 間隔 300ms（< 2×200ms パディング）の隣接区間。旧実装は 2100-2300ms 帯を
        // 二重に切り出し、境界の語が二重転写され得た。開始を前区間末尾でクランプする。
        let r = padded_sample_ranges(&[(1000, 2100), (2400, 3000)], 10_000 * SPMS);
        assert_eq!(
            r,
            vec![
                (800 * SPMS, 2300 * SPMS),
                (2300 * SPMS, 3200 * SPMS), // 2200(=2400-200) でなく前区間末尾 2300 から
            ]
        );
        // 互いに素（重複サンプルなし）。
        assert!(r[0].1 <= r[1].0);
    }

    #[test]
    fn padded_ranges_clamp_to_total_and_skip_empty() {
        // 末尾クランプ + クランプ後に空になった区間はスキップ。
        let total = 2000 * SPMS;
        let r = padded_sample_ranges(&[(1000, 2500), (2600, 2900)], total);
        assert_eq!(r, vec![(800 * SPMS, total)]);
    }

    #[test]
    fn maps_interior_points() {
        let m = sample_map();
        assert_eq!(filtered_ms_to_original(&m, 0, false), 1000);
        assert_eq!(filtered_ms_to_original(&m, 500, false), 1500);
        assert_eq!(filtered_ms_to_original(&m, 2500, true), 8500);
    }

    #[test]
    fn boundary_start_goes_next_end_stays_prev() {
        // filtered=2000 は区間境界。開始は次区間先頭、終了は前区間末尾へ。
        let m = sample_map();
        assert_eq!(filtered_ms_to_original(&m, 2000, false), 8000);
        assert_eq!(filtered_ms_to_original(&m, 2000, true), 3000);
    }

    #[test]
    fn segment_ending_at_boundary_does_not_cross_silence() {
        // 回帰テスト: 旧実装は終了 2000ms を次区間(8000)へ飛ばし無音をまたいでいた。
        let m = sample_map();
        let start = filtered_ms_to_original(&m, 1500, false); // orig 1000+1500
        let end = filtered_ms_to_original(&m, 2000, true); // 区間境界→前区間末尾
        assert_eq!((start, end), (2500, 3000));
        assert!(end - start < 5000, "終了が無音ギャップをまたいでいる");
    }

    #[test]
    fn clamps_beyond_filtered_length() {
        // filtered 長(3500)超は最終区間末尾(9500)へクランプ。
        let m = sample_map();
        assert_eq!(filtered_ms_to_original(&m, 5000, true), 9500);
        assert_eq!(filtered_ms_to_original(&m, 5000, false), 9500);
    }

    #[test]
    fn empty_map_is_identity() {
        assert_eq!(filtered_ms_to_original(&[], 1234, false), 1234);
        assert_eq!(filtered_ms_to_original(&[], 1234, true), 1234);
    }

    #[test]
    fn remap_is_monotonic_nondecreasing() {
        let m = sample_map();
        for at_end in [false, true] {
            let mut prev = 0u64;
            for t in 0..=4000 {
                let v = filtered_ms_to_original(&m, t, at_end);
                assert!(
                    v >= prev,
                    "non-monotonic at t={t} (at_end={at_end}): {v} < {prev}"
                );
                prev = v;
            }
        }
    }

    /// Same audio as `sample_map`, but with the 1 s silence gap `concat_ranges` inserts:
    /// filtered 0-2000 → orig 1000-3000, gap 2000-3000, filtered 3000-4500 → orig 8000-9500.
    fn gap_map() -> Vec<TimeSpan> {
        vec![
            TimeSpan {
                filtered_start_ms: 0,
                orig_start_ms: 1000,
                dur_ms: 2000,
            },
            TimeSpan {
                filtered_start_ms: 3000,
                orig_start_ms: 8000,
                dur_ms: 1500,
            },
        ]
    }

    #[test]
    fn gap_start_snaps_to_next_span_end_snaps_to_previous() {
        let m = gap_map();
        // Inside the inserted gap: a segment start belongs to the next utterance, a segment end
        // to the previous one. Neither may land in the removed 3000-8000 silence.
        assert_eq!(filtered_ms_to_original(&m, 2500, false), 8000);
        assert_eq!(filtered_ms_to_original(&m, 2500, true), 3000);
        // Gap edges behave like the old contiguous boundary. The gap's first sample is an exact
        // hit whisper can produce (10 ms units); a start there must not stay at the previous end.
        assert_eq!(filtered_ms_to_original(&m, 2000, false), 8000);
        assert_eq!(filtered_ms_to_original(&m, 2000, true), 3000);
        assert_eq!(filtered_ms_to_original(&m, 3000, false), 8000);
        assert_eq!(filtered_ms_to_original(&m, 3000, true), 3000);
        // Interior points are unaffected.
        assert_eq!(filtered_ms_to_original(&m, 3500, false), 8500);
        assert_eq!(filtered_ms_to_original(&m, 4500, true), 9500);
    }

    #[test]
    fn gap_remap_is_monotonic_nondecreasing() {
        let m = gap_map();
        for at_end in [false, true] {
            let mut prev = 0u64;
            for t in 0..=5000 {
                let v = filtered_ms_to_original(&m, t, at_end);
                assert!(
                    v >= prev,
                    "non-monotonic at t={t} (at_end={at_end}): {v} < {prev}"
                );
                prev = v;
            }
        }
    }

    #[test]
    fn gap_only_segment_keeps_its_duration_on_the_next_span() {
        let m = gap_map();
        // Both endpoints inside the inserted gap: start would snap to 8000, end back to 3000.
        // The segment is re-anchored at the next span with whisper's own 600 ms duration, so
        // SRT cues and overlap-based speaker assignment still see a real interval.
        assert_eq!(remap_segment(&m, 2200, 2800), (8000, 8600));
        // A zero-length whisper segment stays zero-length (unchanged behaviour without gaps).
        assert_eq!(remap_segment(&m, 2500, 2500), (8000, 8000));
        // The carried duration is clamped to the next span.
        let short_next = vec![
            TimeSpan {
                filtered_start_ms: 0,
                orig_start_ms: 1000,
                dur_ms: 2000,
            },
            TimeSpan {
                filtered_start_ms: 3000,
                orig_start_ms: 8000,
                dur_ms: 300,
            },
        ];
        assert_eq!(remap_segment(&short_next, 2100, 2900), (8000, 8300));
        // Segments that touch speech on either side are unaffected.
        assert_eq!(remap_segment(&m, 1500, 2500), (2500, 3000));
        assert_eq!(remap_segment(&m, 2500, 3500), (8000, 8500));
        assert_eq!(remap_segment(&m, 500, 4000), (1500, 9000));
    }

    #[test]
    fn remapped_segments_never_invert() {
        let m = gap_map();
        for s in (0..=5000).step_by(50) {
            for e in (s..=5000).step_by(50) {
                let (os, oe) = remap_segment(&m, s, e);
                assert!(os <= oe, "inverted: [{s},{e}] -> [{os},{oe}]");
            }
        }
    }

    #[test]
    fn concat_inserts_silence_between_separated_ranges_only() {
        let pcm: Vec<f32> = (0..10_000 * SPMS).map(|i| i as f32).collect();
        let gap = VAD_GAP_MS as usize * SPMS;
        // Three ranges: the second is adjacent to the first (clamped padding), the third is not.
        let ranges = [
            (800 * SPMS, 2300 * SPMS),
            (2300 * SPMS, 3200 * SPMS),
            (5000 * SPMS, 6000 * SPMS),
        ];
        let (filtered, map) = concat_ranges(&pcm, &ranges);

        let speech: usize = ranges.iter().map(|&(a, b)| b - a).sum();
        assert_eq!(filtered.len(), speech + gap, "exactly one gap");
        assert_eq!(map.len(), 3);
        assert_eq!(map[0].filtered_start_ms, 0);
        assert_eq!(map[1].filtered_start_ms, 1500, "adjacent range: no gap");
        assert_eq!(
            map[2].filtered_start_ms,
            1500 + 900 + VAD_GAP_MS,
            "separated range: after the gap"
        );
        assert_eq!((map[2].orig_start_ms, map[2].dur_ms), (5000, 1000));
        // The gap is digital silence and the speech samples are copied verbatim.
        let gap_start = 2400 * SPMS;
        assert!(filtered[gap_start..gap_start + gap]
            .iter()
            .all(|&x| x == 0.0));
        assert_eq!(filtered[gap_start + gap], (5000 * SPMS) as f32);
        assert_eq!(filtered[0], (800 * SPMS) as f32);
    }

    #[test]
    fn concat_without_ranges_is_empty() {
        let pcm = vec![0.5f32; 16_000];
        let (filtered, map) = concat_ranges(&pcm, &[]);
        assert!(filtered.is_empty());
        assert!(map.is_empty());
    }
}
