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

/// Decoder choices measured by the public-audio evaluation harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodingStrategy {
    Greedy,
    BeamSearch5,
}

impl DecodingStrategy {
    fn sampling(self) -> SamplingStrategy {
        match self {
            Self::Greedy => SamplingStrategy::Greedy { best_of: 1 },
            Self::BeamSearch5 => SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            },
        }
    }
}

/// Keep the file default conservative until measured gains justify the cost (Issue #77).
pub const FILE_DECODING: DecodingStrategy = DecodingStrategy::Greedy;

/// Opt-in language selection stored by Settings and queued jobs, never sent to Whisper.
pub const MIXED_LANGUAGE_MODE: &str = "mixed";

/// Resolve automatic mode before constructing decoder parameters so Whisper cannot
/// select an unsupported language. Explicit language choices do not run detection.
fn resolve_language(
    language: Option<&str>,
    detect: impl FnOnce() -> Result<Vec<f32>>,
) -> Result<&str> {
    match language {
        None | Some("" | "auto") => select_meeting_language(&detect()?),
        Some(language) => Ok(language),
    }
}

fn select_meeting_language(probabilities: &[f32]) -> Result<&'static str> {
    let probability = |language| {
        whisper_rs::get_lang_id(language)
            .and_then(|id| probabilities.get(id as usize))
            .copied()
            .filter(|p| p.is_finite() && *p >= 0.0 && *p <= 1.0)
            .ok_or_else(|| CoreError::Model("Invalid speech language probabilities".into()))
    };
    let japanese = probability("ja")?;
    let english = probability("en")?;
    if japanese == 0.0 && english == 0.0 {
        return Err(CoreError::Model(
            "No Japanese or English language evidence".into(),
        ));
    }
    Ok(if japanese > english { "ja" } else { "en" })
}

fn configure_decoder<'a, 'b>(params: &mut FullParams<'a, 'b>, language: &'a str) {
    params.set_language(Some(language));
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
    require_vad: bool,
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
            require_vad: false,
        })
    }

    /// Require successful VAD on every call, including after a cached file is removed.
    /// Live workers use this when admitting quiet tails that must not reach raw Whisper.
    pub fn with_required_vad(mut self) -> Self {
        self.require_vad = true;
        self
    }
}

impl SttEngine for WhisperStt {
    fn transcribe(&self, pcm16k_mono: &[f32], language: Option<&str>) -> Result<Transcript> {
        // FFI 例外シールド: whisper.cpp（C++）の推論中の例外（メモリ枯渇の bad_alloc 等）を
        // Err に変換。シールド無しだと例外が tokio の catch_unwind に達してプロセスごと
        // abort する（docs/error.md の実クラッシュ）。
        crate::ffi_guard::guard("文字起こし (whisper)", || {
            self.transcribe_inner(pcm16k_mono, language, DecodingStrategy::Greedy, None)
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
        self.transcribe_with_decoding(pcm16k_mono, language, FILE_DECODING, on_pct)
    }

    /// Explicit decoding for offline comparisons, with the same VAD and FFI protection.
    /// Live transcription uses `SttEngine::transcribe`, which always selects greedy.
    pub fn transcribe_with_decoding(
        &self,
        pcm16k_mono: &[f32],
        language: Option<&str>,
        decoding: DecodingStrategy,
        on_pct: Option<&dyn Fn(i32)>,
    ) -> Result<Transcript> {
        crate::ffi_guard::guard("文字起こし (whisper)", || {
            self.transcribe_inner(pcm16k_mono, language, decoding, on_pct)
        })?
    }
}

impl WhisperStt {
    fn transcribe_inner(
        &self,
        pcm16k_mono: &[f32],
        language: Option<&str>,
        decoding: DecodingStrategy,
        on_pct: Option<&dyn Fn(i32)>,
    ) -> Result<Transcript> {
        if language == Some(MIXED_LANGUAGE_MODE) {
            return self.transcribe_mixed_inner(pcm16k_mono, decoding, on_pct);
        }
        self.transcribe_single_inner(pcm16k_mono, language, decoding, on_pct, self.require_vad)
    }

    fn transcribe_mixed_inner(
        &self,
        pcm: &[f32],
        decoding: DecodingStrategy,
        on_pct: Option<&dyn Fn(i32)>,
    ) -> Result<Transcript> {
        let vad = self
            .vad_model_path
            .as_ref()
            .ok_or_else(|| CoreError::Model("VAD model is required".into()))?;
        let ranges = vad_sample_ranges(&vad.to_string_lossy(), pcm)?;
        let windows = mixed_language_windows(&ranges, pcm.len());
        let mut segments = Vec::new();
        if let Some(cb) = on_pct {
            cb(0);
        }
        for (start, end) in windows {
            let report = |pct| {
                if let Some(cb) = on_pct {
                    cb(window_progress(start, end, pcm.len(), pct));
                }
            };
            let callback = on_pct.map(|_| &report as &dyn Fn(i32));
            // A fresh decoding call re-detects language without restoring rolling text history.
            // The model is shared; each window's state and filtered PCM are dropped before the next.
            let transcript =
                self.transcribe_single_inner(&pcm[start..end], None, decoding, callback, true)?;
            let offset_ms = start as u64 / 16;
            for mut segment in transcript.segments {
                segment.start_ms += offset_ms;
                segment.end_ms += offset_ms;
                segments.push(segment);
            }
            report(100);
        }
        if let Some(cb) = on_pct {
            cb(100);
        }
        Ok(Transcript {
            language: None,
            segments,
        })
    }

    fn transcribe_single_inner(
        &self,
        pcm16k_mono: &[f32],
        language: Option<&str>,
        decoding: DecodingStrategy,
        on_pct: Option<&dyn Fn(i32)>,
        require_vad: bool,
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
                Err(error) if require_vad => return Err(error),
                Err(error) => {
                    eprintln!("stt vad: filtering failed, using raw audio: {error}");
                    (Cow::Borrowed(pcm16k_mono), None)
                }
            },
            None if require_vad => {
                return Err(CoreError::Model("VAD model is required".into()));
            }
            None => (Cow::Borrowed(pcm16k_mono), None),
        };

        let selected_language = resolve_language(language, || {
            // Match Whisper's default thread cap. Native Auto already runs this encoder
            // pass; using its probability vector lets us restrict the winning language.
            let threads = std::thread::available_parallelism()
                .map(|count| count.get().min(4))
                .unwrap_or(1);
            // Detection consumes the first 30 seconds. Bound the extra mel preparation:
            // whisper-rs full() requires nonempty PCM and recomputes mel internally.
            let detection_pcm = &pcm[..pcm.len().min(30 * 16_000)];
            state
                .pcm_to_mel(detection_pcm, threads)
                .map_err(|e| CoreError::Model(format!("language mel: {e:?}")))?;
            let (_, probabilities) = state
                .lang_detect(0, threads)
                .map_err(|e| CoreError::Model(format!("language detection: {e:?}")))?;
            Ok(probabilities)
        })?;
        let mut params = FullParams::new(decoding.sampling());
        configure_decoder(&mut params, selected_language);
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
            language: Some(selected_language.to_string()),
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

/// Re-detect language across a pause of at least 750 ms before VAD padding.
const MIXED_LANGUAGE_PAUSE_MS: u64 = 750;

fn mixed_language_windows(ranges: &[(usize, usize)], total: usize) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let retained_gap = (MIXED_LANGUAGE_PAUSE_MS - 2 * VAD_PAD_MS) as usize * 16;
    let mut windows = Vec::new();
    let mut start = 0;
    for pair in ranges.windows(2) {
        if pair[1].0.saturating_sub(pair[0].1) >= retained_gap {
            let cut = pair[0].1 + (pair[1].0 - pair[0].1) / 2;
            windows.push((start, cut));
            start = cut;
        }
    }
    windows.push((start, total));
    windows
}

fn window_progress(start: usize, end: usize, total: usize, pct: i32) -> i32 {
    if total == 0 {
        return 100;
    }
    ((start as u128 * 100 + (end - start) as u128 * pct.clamp(0, 100) as u128) / total as u128)
        as i32
}

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

/// Give very quiet speech a usable level for Silero without changing Whisper's audio.
/// Use the 90th percentile of nonzero one-second block RMS values: digital silence
/// must not hide a short utterance, and an isolated loud sound must not prevent gain
/// on an otherwise quiet track. Normal-level audio is borrowed without a PCM copy.
/// The gain is capped at 16x (24 dB); VAD still decides whether speech is present.
pub fn vad_analysis_pcm(pcm: &[f32]) -> Cow<'_, [f32]> {
    let mut levels: Vec<f32> = pcm
        .chunks(SAMPLE_RATE_F as usize)
        .map(|block| (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt())
        .filter(|rms| *rms > 0.0)
        .collect();
    if levels.is_empty() {
        return Cow::Borrowed(pcm);
    }
    levels.sort_unstable_by(f32::total_cmp);
    let reference = levels[(levels.len() - 1) * 9 / 10];
    if !reference.is_finite() || reference >= 0.01 {
        return Cow::Borrowed(pcm);
    }
    let gain = (0.05 / reference).min(16.0);
    Cow::Owned(pcm.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect())
}

/// Silero VAD で発話区間だけを抜き出した PCM と、filtered→original の時刻マップを返す。
fn vad_filter(model_path: &str, pcm: &[f32]) -> Result<(Vec<f32>, Vec<TimeSpan>)> {
    let ranges = vad_sample_ranges(model_path, pcm)?;
    Ok(concat_ranges(pcm, &ranges))
}

/// Detect and pad original-time speech ranges with the same policy for both modes.
fn vad_sample_ranges(model_path: &str, pcm: &[f32]) -> Result<Vec<(usize, usize)>> {
    let mut vctx = WhisperVadContext::new(model_path, WhisperVadContextParams::new())
        .map_err(|e| CoreError::Model(format!("vad ctx: {e:?}")))?;
    let segs = {
        // Release the analysis copy before allocating the filtered Whisper input.
        // Spans refer to the same sample indices; concat_ranges below reads the original PCM.
        let analysis = vad_analysis_pcm(pcm);
        vctx.segments_from_samples(WhisperVadParams::new(), &analysis)
            .map_err(|e| CoreError::Model(format!("vad segments: {e:?}")))?
    };

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

    Ok(padded_sample_ranges(&segs_ms, pcm.len()))
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
    fn mixed_windows_split_at_long_pauses_without_cutting_speech() {
        let ranges = [(800, 2400), (4000, 8000), (24000, 32000)];
        let windows = mixed_language_windows(&ranges, 40000);
        assert_eq!(windows, [(0, 16000), (16000, 40000)]);
        for (start, end) in ranges {
            assert_eq!(
                windows
                    .iter()
                    .filter(|(a, b)| *a <= start && end <= *b)
                    .count(),
                1
            );
        }
        assert_eq!(windows.iter().map(|(a, b)| b - a).sum::<usize>(), 40000);
    }

    #[test]
    fn mixed_windows_handle_silence_and_the_padded_gap_threshold() {
        assert!(mixed_language_windows(&[], 48000).is_empty());
        assert_eq!(
            mixed_language_windows(&[(0, 16000), (21599, 32000)], 32000),
            [(0, 32000)]
        );
        assert_eq!(
            mixed_language_windows(&[(0, 16000), (21600, 32000)], 32000),
            [(0, 18800), (18800, 32000)]
        );
    }

    #[test]
    fn mixed_progress_does_not_restart_at_window_boundaries() {
        let progress: Vec<_> = [(0, 100), (100, 400)]
            .into_iter()
            .flat_map(|(start, end)| [0, 50, 100].map(|pct| window_progress(start, end, 400, pct)))
            .collect();
        assert_eq!(progress, [0, 12, 25, 25, 62, 100]);
        assert_eq!(window_progress(100, 400, 400, -1), 25);
        assert_eq!(window_progress(100, 400, 400, 101), 100);
        assert_eq!(window_progress(0, 0, 0, 0), 100);
    }

    #[test]
    fn vad_analysis_recovers_quiet_level_despite_an_isolated_loud_sound() {
        let mut pcm = vec![0.002; 16_000 * 20];
        pcm[16_000 * 10..16_000 * 11].fill(1.0);
        let analysis = vad_analysis_pcm(&pcm);
        assert_eq!(analysis.len(), pcm.len());
        assert!(
            analysis[0] >= 0.03,
            "quiet speech needs useful VAD input level"
        );
        assert!(analysis.iter().all(|s| s.abs() <= 1.0));
        assert_eq!(pcm[0], 0.002, "Whisper must retain the original samples");
        assert_eq!(pcm[16_000 * 10], 1.0);
    }

    #[test]
    fn vad_analysis_leaves_normal_audio_and_silence_borrowed() {
        for pcm in [vec![], vec![0.0; 16_000], vec![0.05; 16_000]] {
            assert!(matches!(vad_analysis_pcm(&pcm), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn vad_analysis_bounds_gain_and_handles_partial_blocks() {
        let pcm = vec![0.0001; 731];
        let analysis = vad_analysis_pcm(&pcm);
        assert_eq!(analysis.len(), pcm.len());
        assert!(analysis[0] > pcm[0]);
        assert!(
            analysis[0] <= pcm[0] * 16.0,
            "never amplify arbitrarily quiet noise without a bound"
        );
    }

    #[test]
    fn vad_analysis_silence_padding_does_not_hide_quiet_speech() {
        let mut pcm = vec![0.0; 16_000 * 20];
        pcm[..16_000].fill(0.002);
        let analysis = vad_analysis_pcm(&pcm);
        assert!(analysis[0] >= 0.03);
        assert!(analysis[16_000..].iter().all(|&s| s == 0.0));
    }

    /// Opt-in real-model check. The fixture is public FLEURS speech, never a meeting recording.
    /// See ADR-0035 for the pinned fixture and environment variables.
    #[test]
    #[ignore = "requires the pinned public WAV, local Whisper/VAD models, and GPU access"]
    fn vad_keeps_attenuated_public_speech_and_rejects_silence() {
        use sha2::{Digest, Sha256};
        let audio = std::env::var("MOJIROKU_TEST_SPEECH_WAV").expect("MOJIROKU_TEST_SPEECH_WAV");
        let models =
            PathBuf::from(std::env::var("MOJIROKU_TEST_MODELS").expect("MOJIROKU_TEST_MODELS"));
        let bytes = std::fs::read(&audio).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "697876fbd65b56e578f94a0eed8fa23ef2f0afbb149c83f402135448abed344e",
            "use the pinned public fixture"
        );
        let mut pcm = crate::audio::decode_to_pcm16k_mono(audio).unwrap();
        let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len() as f32).sqrt();
        for sample in &mut pcm {
            *sample *= 0.0003 / rms;
        }
        let mut engine =
            WhisperStt::load(models.join(crate::models::DEFAULT_WHISPER_MODEL), None).unwrap();
        let raw = engine.transcribe(&pcm, None).unwrap();
        assert!(engine.transcribe(&pcm, Some(MIXED_LANGUAGE_MODE)).is_err());
        let words = |t: &Transcript| {
            t.segments
                .iter()
                .flat_map(|s| s.text.split_whitespace())
                .map(|word| {
                    word.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert!(
            words(&raw).split_whitespace().count() >= 10,
            "the decoder can hear this fixture"
        );
        engine = engine.with_required_vad();
        engine.vad_model_path = Some(models.join(crate::models::DEFAULT_VAD_MODEL));
        let filtered = engine.transcribe(&pcm, None).unwrap();
        let mixed = engine.transcribe(&pcm, Some(MIXED_LANGUAGE_MODE)).unwrap();
        assert_eq!(words(&mixed), words(&filtered));
        assert_eq!(
            words(&filtered),
            words(&raw),
            "VAD must not discard the audible sentence"
        );
        assert!(filtered
            .segments
            .iter()
            .all(|s| s.start_ms <= s.end_ms && s.end_ms <= 12_440));
        assert!(engine
            .transcribe(&vec![0.0; 16_000 * 60], None)
            .unwrap()
            .segments
            .is_empty());
        assert!(engine
            .transcribe(&vec![0.0; 16_000 * 60], Some(MIXED_LANGUAGE_MODE))
            .unwrap()
            .segments
            .is_empty());

        let mut noise_state = 17_u64;
        let noise: Vec<f32> = (0..16_000 * 20)
            .map(|_| {
                noise_state = noise_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1);
                (noise_state >> 32) as i32 as f32 / i32::MAX as f32 * 0.0003
            })
            .collect();
        for mode in [None, Some(MIXED_LANGUAGE_MODE)] {
            assert!(engine.transcribe(&noise, mode).unwrap().segments.is_empty());
        }

        use std::io::Write;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let broken_vad = std::env::temp_dir().join(format!(
            "mojiroku-broken-vad-{}-{stamp}.bin",
            std::process::id()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&broken_vad)
            .unwrap()
            .write_all(b"invalid VAD model")
            .unwrap();
        engine.vad_model_path = Some(broken_vad.clone());
        let corrupt = engine.transcribe(&pcm, None);
        let mixed_corrupt = engine.transcribe(&pcm, Some(MIXED_LANGUAGE_MODE));
        std::fs::remove_file(&broken_vad).unwrap();
        assert!(
            corrupt.is_err(),
            "required VAD must not fall back to raw PCM"
        );
        assert!(mixed_corrupt.is_err(), "mixed mode requires successful VAD");
        assert!(
            engine.transcribe(&pcm, None).is_err(),
            "removed VAD must fail closed"
        );
        assert!(engine.transcribe(&pcm, Some(MIXED_LANGUAGE_MODE)).is_err());
        engine.vad_model_path = None;
        assert!(
            engine.transcribe(&pcm, None).is_err(),
            "missing VAD must fail closed"
        );
    }

    #[test]
    fn decoder_disables_rolling_text_history() {
        // no_context alone only clears history at the start of full(), not between its
        // audio windows. Pin the separate history budget against dependency defaults.
        for language in ["ja", "en"] {
            for decoding in [DecodingStrategy::Greedy, DecodingStrategy::BeamSearch5] {
                let mut params = FullParams::new(decoding.sampling());
                configure_decoder(&mut params, language);
                let debug = format!("{params:?}");
                assert!(debug.contains("n_max_text_ctx: 0,"), "{debug}");
            }
        }
    }

    #[test]
    fn decoder_choices_reach_whisper_with_the_requested_search_width() {
        let greedy = format!("{:?}", FullParams::new(DecodingStrategy::Greedy.sampling()));
        let beam = format!(
            "{:?}",
            FullParams::new(DecodingStrategy::BeamSearch5.sampling())
        );
        assert!(greedy.contains("strategy: 0,"), "{greedy}");
        assert!(greedy.contains("best_of: 1"), "{greedy}");
        assert!(beam.contains("strategy: 1,"), "{beam}");
        assert!(beam.contains("beam_size: 5"), "{beam}");
    }

    /// This short public Japanese excerpt makes unrestricted detection select Chinese.
    /// Restricting candidates does not promise that an ambiguous excerpt selects Japanese.
    #[test]
    #[ignore = "requires pinned public short-tail WAV and local GPU/models; see ADR-0039"]
    fn auto_language_restricts_an_ambiguous_public_tail() {
        use sha2::{Digest, Sha256};
        let audio = std::env::var("MOJIROKU_TEST_LANGUAGE_WAV").unwrap();
        let bytes = std::fs::read(&audio).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            "9273720d8c41f5f49729aee40551d5d5a3643bd98fed5e4dea68d14c1f48f50c"
        );
        let pcm = crate::audio::decode_to_pcm16k_mono(&audio).unwrap();
        let tail = &pcm[101_760..117_760];
        let models = PathBuf::from(std::env::var("MOJIROKU_TEST_MODELS").unwrap());
        let vad = models.join(crate::models::DEFAULT_VAD_MODEL);
        let engine = WhisperStt::load(
            models.join(crate::models::DEFAULT_WHISPER_MODEL),
            Some(vad.clone()),
        )
        .unwrap()
        .with_required_vad();
        crate::ffi_guard::guard("public short-language detection", || {
            let (filtered, _) = vad_filter(&vad.to_string_lossy(), tail).unwrap();
            assert!(!filtered.is_empty());
            let mut state = engine.ctx.create_state().unwrap();
            state.pcm_to_mel(&filtered, 4).unwrap();
            let (unrestricted, _) = state.lang_detect(0, 4).unwrap();
            assert_eq!(whisper_rs::get_lang_str(unrestricted), Some("zh"));
        })
        .unwrap();
        let transcript = engine.transcribe(tail, None).unwrap();
        assert!(matches!(transcript.language.as_deref(), Some("ja" | "en")));
    }

    fn language_probabilities(japanese: f32, english: f32) -> Vec<f32> {
        let mut probabilities = vec![0.0; whisper_rs::get_lang_max_id() as usize + 1];
        probabilities[whisper_rs::get_lang_id("ja").unwrap() as usize] = japanese;
        probabilities[whisper_rs::get_lang_id("en").unwrap() as usize] = english;
        probabilities
    }

    #[test]
    fn auto_language_ignores_higher_unsupported_language_probabilities() {
        for unsupported in ["ko", "zh"] {
            for (ja, en, expected) in [(0.08, 0.02, "ja"), (0.02, 0.08, "en")] {
                let mut probabilities = language_probabilities(ja, en);
                probabilities[whisper_rs::get_lang_id(unsupported).unwrap() as usize] = 0.9;
                for automatic in [None, Some("auto"), Some("")] {
                    let language =
                        resolve_language(automatic, || Ok(probabilities.clone())).unwrap();
                    assert_eq!(language, expected);
                    let mut params = FullParams::new(DecodingStrategy::Greedy.sampling());
                    configure_decoder(&mut params, language);
                    assert!(!format!("{params:?}").contains("language: 0x0"));
                }
            }
        }
    }

    #[test]
    fn explicit_language_does_not_detect_or_override_the_choice() {
        for language in ["ja", "en"] {
            assert_eq!(
                resolve_language(Some(language), || panic!("must not detect")).unwrap(),
                language
            );
        }
    }

    #[test]
    fn invalid_language_evidence_fails_without_unconstrained_fallback() {
        assert!(select_meeting_language(&[]).is_err());
        for (ja, en) in [
            (0.0, 0.0),
            (f32::NAN, 0.4),
            (0.4, f32::INFINITY),
            (-0.1, 0.4),
            (0.4, 1.1),
        ] {
            assert!(select_meeting_language(&language_probabilities(ja, en)).is_err());
        }
        assert!(resolve_language(None, || Err(CoreError::Model("detect failed".into()))).is_err());
        assert_eq!(
            select_meeting_language(&language_probabilities(0.5, 0.5)).unwrap(),
            "en"
        );
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
