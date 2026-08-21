//! システム音声キャプチャ（ScreenCaptureKit）。会議モード（Phase 7 / ADR-0017）の中核で、
//! 会議相手（Zoom/Meet/Teams のリモート参加者）の音声をローカルで取得する。
//!
//! `SCStream` とその周辺オブジェクトは専用スレッドに閉じ込め、managed state には停止シグナル・
//! 蓄積バッファ・無音監視だけを持つ（`mic.rs` と対称）。OS/ScreenCaptureKit 依存なので
//! UI 非依存の `mojiroku-core` ではなく `src-tauri` 側に置く。
//!
//! 重要（ADR-0017、スパイク `spikes/meeting-audio/` で実測済み）:
//! - `SCShareableContent::get()` が TCC（画面とシステムオーディオ収録）の許可ゲート点。
//! - **`get()` 成功 ≠ 録れている**。会期中の全ゼロ PCM（macOS 26.5 バグ / Sequoia 月次失効）は
//!   `get()` では検出できず、コールバックも発火し続ける。よって RMS（peak/直近の非無音時刻）を
//!   監視し、上位層が「N 秒 RMS≈0」を見て無音警告を出せるようにする。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use screencapturekit::prelude::*;

use crate::audio::spool::{SharedPcm, WavSpoolWriter};

/// spool への書き出し周期・末尾保持（mic.rs と同値。根拠はそちらのコメント参照）。
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const KEEP_TAIL_SECS: usize = 30;

/// 要求キャプチャ設定。SCK は要求どおりの形で来ないことがある（native 48k stereo f32 等）が、
/// 蓄積後は要求レートとして扱う。スパイクの知見: 実測レート推定で WAV を書くとピッチがずれて
/// 「壊れている」と誤読される。レートのズレは上位の診断（frames/elapsed）に委ねる。
const CAPTURE_SAMPLE_RATE: u32 = 48_000;
const CAPTURE_CHANNELS: i32 = 2;

/// 非無音とみなす RMS 閾値（スパイクで実音声 0.36 / 完全無音 0.00000 を観測）。会議停止時の
/// system トラック採否判定（`commands::recording`）も同じ無音フロアを共有するため crate 公開。
pub(crate) const SILENCE_RMS_THRESHOLD: f32 = 1e-4;

/// little-endian な生バイト列を f32 サンプル列へ再解釈する（端数バイトは捨てる）。
fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// AudioBufferList 由来の (チャンネル数, f32データ) 群を mono へダウンミックスする。
/// planar（buffer=channel）と interleaved の両対応。スパイクで実音声 RMS 0.36 を確認した経路。
fn downmix_to_mono(bufs: &[(u32, Vec<f32>)]) -> Vec<f32> {
    if bufs.len() > 1 {
        // planar: 各 buffer が 1ch。フレーム長は最小に合わせて平均。
        let frames = bufs.iter().map(|(_, d)| d.len()).min().unwrap_or(0);
        (0..frames)
            .map(|i| bufs.iter().map(|(_, d)| d[i]).sum::<f32>() / bufs.len() as f32)
            .collect()
    } else if let Some((ch, data)) = bufs.first() {
        let ch = (*ch).max(1) as usize;
        if ch <= 1 {
            data.clone()
        } else {
            // interleaved: ch 個ずつ平均
            data.chunks_exact(ch)
                .map(|fr| fr.iter().sum::<f32>() / ch as f32)
                .collect()
        }
    } else {
        Vec::new()
    }
}

/// mono サンプル列の RMS（二乗平均平方根）。非無音判定に使う。
pub(crate) fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }
}

/// interleaved な f32 PCM を mono へ落とす（再生用ミックスの前処理）。ch<=1 はそのまま。
pub fn interleaved_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(ch)
        .map(|fr| fr.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// 簡易リニア補間リサンプル（mono）。**再生用のミックス整列にだけ使う**（高品質は不要）。
/// 文字起こしは per-track の元 WAV を使うのでここの品質は会議の文字起こしに影響しない。
pub fn resample_linear_mono(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == 0 || to_rate == 0 || from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let i0 = src.floor() as usize;
        let frac = (src - i0 as f64) as f32;
        let s0 = input.get(i0).copied().unwrap_or(0.0);
        let s1 = input.get(i0 + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
    }
    out
}

/// 同レートの 2 つの mono トラックを加算ミックス（長さは max・クリップ防止にクランプ）。
/// 再生用の粗いミックス。長尺ではδ（開始ズレ）/クロックドリフトで両者が徐々にズレるが
/// 視聴用途では許容（ADR-0017。文字起こしは per-track で正確）。
pub fn mix_mono(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| {
            let s = a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0);
            s.clamp(-1.0, 1.0)
        })
        .collect()
}

/// 再生用ミックスのため、**所有**の PCM を mono・指定レートへ変換する。既に mono かつ同レートなら
/// 一切複製せず元 Vec をそのまま返し、中間複製を作らない（mic/system は実測 48k mono なので通常は
/// 複製ゼロ）。長尺会議の停止時に生 Vec＋mono 化＋resample＋mixed が同時に乗るメモリピークを抑える。
pub fn to_playback_mono(samples: Vec<f32>, channels: u16, from_rate: u32, to_rate: u32) -> Vec<f32> {
    // mono 化（既に mono なら所有のまま素通し）。2ch 以上のときのみ新 Vec を作り、元 samples は drop。
    let mono = if channels <= 1 {
        samples
    } else {
        interleaved_to_mono(&samples, channels)
    };
    // レート変換（同レートなら所有のまま素通し）。
    if from_rate == to_rate {
        mono
    } else {
        resample_linear_mono(&mono, from_rate, to_rate)
    }
}

/// 無音監視（RMS ウォッチドッグの土台）。音声コールバックから lock-free に更新し、上位層が
/// ポーリングして「会期中の全ゼロ化（26.5 バグ / 月次失効）」を警告判断する（ADR-0017）。
pub struct SilenceMonitor {
    /// 音声コールバックが一度でも発火したか。
    any_callback: AtomicBool,
    /// 観測した最大 RMS（×1e6 した整数で lock-free 保持）。
    peak_rms_micro: AtomicU64,
    /// 直近で「非無音」を観測したキャプチャ開始からの経過 ms（0 = まだ無い）。
    last_nonsilent_ms: AtomicU64,
}

impl SilenceMonitor {
    fn new() -> Self {
        Self {
            any_callback: AtomicBool::new(false),
            peak_rms_micro: AtomicU64::new(0),
            last_nonsilent_ms: AtomicU64::new(0),
        }
    }

    /// 音声コールバックから 1 ブロック分の RMS・経過 ms を記録する。
    fn record(&self, rms: f32, elapsed_ms: u64) {
        self.any_callback.store(true, Ordering::Relaxed);
        let micro = (rms.max(0.0) * 1e6) as u64;
        self.peak_rms_micro.fetch_max(micro, Ordering::Relaxed);
        if rms > SILENCE_RMS_THRESHOLD {
            self.last_nonsilent_ms.store(elapsed_ms, Ordering::Relaxed);
        }
    }

    /// 観測した最大 RMS。
    pub fn peak_rms(&self) -> f32 {
        self.peak_rms_micro.load(Ordering::Relaxed) as f32 / 1e6
    }

    /// 音声コールバックが発火したか。
    pub fn any_callback(&self) -> bool {
        self.any_callback.load(Ordering::Relaxed)
    }

    /// 直近で非無音を観測した経過 ms（0 = 未観測）。上位の無音警告ロジック用。
    // 後続増分の RMS ウォッチドッグ（監視スレッドが「経過 - last_nonsilent > N 秒」を検出して
    // meeting://audio-silent を emit）で使う。現時点では未配線のため allow。
    #[allow(dead_code)]
    pub fn last_nonsilent_ms(&self) -> u64 {
        self.last_nonsilent_ms.load(Ordering::Relaxed)
    }
}

/// キャプチャ中セッション。`SCStream` はワーカースレッド内のみに生き、ここには持たない。
pub struct CaptureSession {
    stop_tx: Sender<()>,
    samples: Arc<SharedPcm>,
    /// ワーカーが finalize 後に (spool_path, samples_written) を返す。
    result_rx: Receiver<Result<(PathBuf, u64), String>>,
    handle: JoinHandle<()>,
    sample_rate: u32,
    silence: Arc<SilenceMonitor>,
    /// flush 中の IO エラー（チャンク破棄で継続した印）。
    spool_error: Arc<Mutex<Option<String>>>,
}

/// `stop()` の結果。PCM 本体は spool WAV にある（メモリで返さない）。
pub struct SystemStopInfo {
    pub spool_path: PathBuf,
    /// 書き込んだサンプル総数（mono）。0 = 無録音。
    pub samples_written: u64,
    pub sample_rate: u32,
    pub peak_rms: f32,
    /// 音声コールバックが一度でも発火したか。後続の RMS ウォッチドッグ（無音警告）用で
    /// 現時点の呼び出し側は未参照（旧 stop() タプルでも同様に未使用だった）。
    #[allow(dead_code)]
    pub any_callback: bool,
    /// flush 中の IO エラー（部分保存で継続した警告。None = 完全）。
    pub spool_error: Option<String>,
}

/// キャプチャセッションの managed state（同時に 1 つ）。
pub struct SystemAudioState(pub Mutex<Option<CaptureSession>>);

impl SystemAudioState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// TCC（画面とシステムオーディオ収録）の許可状態。`SCShareableContent::get()` が許可ゲート点で、
/// 成功=許可、失敗=未許可とみなす（ADR-0017）。会議モードの起動時プリフライト・更新後の失効検出用。
/// 注意: これは「許可がある」しか保証しない。録れているか（非無音）は別途 RMS で監視すること。
pub fn check_permission() -> bool {
    match SCShareableContent::get() {
        Ok(_) => true,
        Err(e) => {
            eprintln!("[system_audio] 権限未許可または取得失敗: {e:?}");
            false
        }
    }
}

/// システム音声キャプチャ開始。TCC ゲート・stream 構築・start_capture はすべてワーカースレッド内で
/// 完結させ（SCK オブジェクトの Send 制約を回避）、初期化結果だけを oneshot で呼び出し側へ返す。
/// PCM は `spool_path` の WAV へ逐次書き出す。**初期化に失敗した場合は spool を削除**する
/// （空 WAV を残さない）。
pub fn start(state: &SystemAudioState, spool_path: PathBuf) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    if guard.is_some() {
        return Err("error.system_audio.busy".into());
    }

    let mut spool = WavSpoolWriter::create(&spool_path, CAPTURE_SAMPLE_RATE, 1)
        .map_err(|e| format!("spool create: {e}"))?;

    let samples = Arc::new(SharedPcm::new());
    let silence = Arc::new(SilenceMonitor::new());
    let spool_error = Arc::new(Mutex::new(None));
    let spool_error_w = Arc::clone(&spool_error);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    // 初期化（get / stream 構築 / start_capture）の成否をスレッドから返すための oneshot。
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let (result_tx, result_rx) = mpsc::channel::<Result<(PathBuf, u64), String>>();
    const KEEP_TAIL: usize = KEEP_TAIL_SECS * CAPTURE_SAMPLE_RATE as usize; // mono

    let samples_cb = Arc::clone(&samples);
    let shared = Arc::clone(&samples);
    let silence_cb = Arc::clone(&silence);

    let handle = std::thread::spawn(move || {
        let t0 = std::time::Instant::now();

        // TCC ゲート点 + 共有コンテンツ取得。
        let content = match SCShareableContent::get() {
            Ok(c) => c,
            Err(e) => {
                // 誘導文（システム設定 > … で許可）は i18n 辞書側（error.system_audio.permission）が持つ。
                let _ = ready_tx.send(Err(format!("error.system_audio.permission: {e:?}")));
                return;
            }
        };
        let displays = content.displays();
        let display = match displays.first() {
            Some(d) => d,
            None => {
                let _ = ready_tx.send(Err("error.system_audio.no_display".into()));
                return;
            }
        };

        // 表示全体のシステム音声を取得するフィルタ（ウィンドウ除外なし）。
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        // 音声専用のつもりだが SCK は映像を完全には切れない → 2x2 ダミー映像にして捨てる。
        // 自プロセス音声は除外（会議メモアプリ自身の音を録らない）。
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_captures_audio(true)
            .with_sample_rate(CAPTURE_SAMPLE_RATE as i32)
            .with_channel_count(CAPTURE_CHANNELS)
            .with_excludes_current_process_audio(true);

        let mut stream = SCStream::new(&filter, &config);

        // 映像（Screen）は no-op。known-good パターンに合わせ登録する
        // （音声のみで start するとコールバックが来ない実装差異を避ける）。
        stream.add_output_handler(
            |_sample: CMSampleBuffer, _of_type: SCStreamOutputType| {},
            SCStreamOutputType::Screen,
        );

        // 音声（Audio）: AudioBufferList を mono にダウンミックスして蓄積、RMS を監視する。
        stream.add_output_handler(
            move |sample: CMSampleBuffer, of_type: SCStreamOutputType| {
                if !matches!(of_type, SCStreamOutputType::Audio) {
                    return;
                }
                let Some(list) = sample.audio_buffer_list() else {
                    return;
                };
                let bufs: Vec<(u32, Vec<f32>)> = list
                    .iter()
                    .map(|b| (b.number_channels, bytes_to_f32(b.data())))
                    .collect();
                let mono = downmix_to_mono(&bufs);
                silence_cb.record(rms(&mono), t0.elapsed().as_millis() as u64);
                samples_cb.push(&mono);
            },
            SCStreamOutputType::Audio,
        );

        if let Err(e) = stream.start_capture() {
            let _ = ready_tx.send(Err(format!("システム音声キャプチャの開始に失敗: {e:?}")));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        // flush ループ: FLUSH_INTERVAL ごとに末尾 KEEP_TAIL を残して spool へ追記。
        // IO エラー時はチャンクを破棄してキャプチャは継続する（部分保存 > 全損）。
        loop {
            match stop_rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let chunk = shared.take_flush_chunk(KEEP_TAIL);
                    if !chunk.is_empty() {
                        if let Err(e) = spool.append(&chunk).and_then(|_| spool.flush()) {
                            *spool_error_w.lock().unwrap() = Some(e);
                        }
                    }
                }
            }
        }
        let _ = stream.stop_capture(); // 同期停止（以後 push は来ない）
        drop(stream);

        // 最終 drain → finalize。
        let rest = shared.take_flush_chunk(0);
        if !rest.is_empty() {
            if let Err(e) = spool.append(&rest) {
                *spool_error_w.lock().unwrap() = Some(e);
            }
        }
        let _ = result_tx.send(spool.finalize());
    });

    match ready_rx.recv() {
        Ok(Ok(())) => {
            *guard = Some(CaptureSession {
                stop_tx,
                samples,
                result_rx,
                handle,
                sample_rate: CAPTURE_SAMPLE_RATE,
                silence,
                spool_error,
            });
            Ok(())
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            let _ = std::fs::remove_file(&spool_path); // 空 spool を残さない
            Err(e)
        }
        Err(_) => {
            let _ = handle.join();
            let _ = std::fs::remove_file(&spool_path);
            Err("キャプチャスレッドの初期化に失敗しました".into())
        }
    }
}

/// ライブ文字起こし（増分C）用に、進行中キャプチャの mono 共有バッファ・sample_rate を返す。
/// キャプチャ中でなければ None。バッファは spool flush で先頭が解放されるため、読者は
/// `SharedPcm::snapshot_from` の絶対 index で追従する（`live_stt::take_new`）。
pub fn live_handle(state: &SystemAudioState) -> Option<(Arc<SharedPcm>, u32)> {
    let guard = state.0.lock().unwrap();
    guard.as_ref().map(|s| (Arc::clone(&s.samples), s.sample_rate))
}

/// キャプチャ停止。spool WAV を finalize し、パス・サンプル総数・sample_rate・
/// peak RMS・コールバック有無を返す（rename・文字起こしは呼び出し側）。
pub fn stop(state: &SystemAudioState) -> Result<SystemStopInfo, String> {
    let session = state
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("システム音声をキャプチャしていません")?;
    let CaptureSession {
        stop_tx,
        samples: _,
        result_rx,
        handle,
        sample_rate,
        silence,
        spool_error,
    } = session;
    let _ = stop_tx.send(());
    let _ = handle.join();
    let (spool_path, samples_written) = result_rx
        .recv()
        .map_err(|_| "キャプチャワーカーが結果を返しませんでした".to_string())??;
    let spool_error = spool_error.lock().unwrap().take();
    Ok(SystemStopInfo {
        spool_path,
        samples_written,
        sample_rate,
        peak_rms: silence.peak_rms(),
        any_callback: silence.any_callback(),
        spool_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_to_f32_reinterprets_le() {
        let bytes = 1.0_f32.to_le_bytes();
        assert_eq!(bytes_to_f32(&bytes), vec![1.0]);
        // 端数バイト（4 未満の余り）は捨てる。
        let mut v = 0.5_f32.to_le_bytes().to_vec();
        v.push(0xAB);
        assert_eq!(bytes_to_f32(&v), vec![0.5]);
    }

    #[test]
    fn downmix_interleaved_stereo() {
        // 1 buffer / 2ch interleaved: [L0,R0, L1,R1] → 平均
        let data = vec![1.0, -1.0, 0.5, 0.5];
        let mono = downmix_to_mono(&[(2, data)]);
        assert_eq!(mono, vec![0.0, 0.5]);
    }

    #[test]
    fn downmix_planar_stereo() {
        // 2 buffers / 各1ch planar: L=[1,1] R=[-1,0] → 平均 [0, 0.5]
        let mono = downmix_to_mono(&[(1, vec![1.0, 1.0]), (1, vec![-1.0, 0.0])]);
        assert_eq!(mono, vec![0.0, 0.5]);
    }

    #[test]
    fn downmix_mono_passthrough() {
        let mono = downmix_to_mono(&[(1, vec![0.1, 0.2, 0.3])]);
        assert_eq!(mono, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn rms_silence_vs_signal() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0, 0.0, 0.0]), 0.0);
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn silence_monitor_tracks_peak_and_nonsilent() {
        let m = SilenceMonitor::new();
        assert!(!m.any_callback());
        assert_eq!(m.peak_rms(), 0.0);
        assert_eq!(m.last_nonsilent_ms(), 0);

        m.record(0.0, 1000); // 全ゼロ: callback は立つが last_nonsilent は更新しない
        assert!(m.any_callback());
        assert_eq!(m.last_nonsilent_ms(), 0);

        m.record(0.36, 2000); // 非無音
        assert!((m.peak_rms() - 0.36).abs() < 1e-3);
        assert_eq!(m.last_nonsilent_ms(), 2000);

        m.record(0.0, 3000); // 再び無音: peak と last_nonsilent は据え置き
        assert!((m.peak_rms() - 0.36).abs() < 1e-3);
        assert_eq!(m.last_nonsilent_ms(), 2000);
    }

    #[test]
    fn interleaved_to_mono_averages_channels() {
        // 2ch: [L0,R0, L1,R1] → 平均
        assert_eq!(interleaved_to_mono(&[1.0, -1.0, 0.5, 0.5], 2), vec![0.0, 0.5]);
        // mono はそのまま
        assert_eq!(interleaved_to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_linear_mono_basics() {
        // 同レートはそのまま。
        assert_eq!(resample_linear_mono(&[0.1, 0.2, 0.3], 48_000, 48_000), vec![0.1, 0.2, 0.3]);
        // 2 倍アップサンプルは概ね 2 倍の長さ。
        let up = resample_linear_mono(&[0.0, 1.0], 1, 2);
        assert_eq!(up.len(), 4);
        assert_eq!(up[0], 0.0); // 先頭は原点
        // 半分ダウンサンプルは概ね半分。
        assert_eq!(resample_linear_mono(&[0.0, 1.0, 0.0, 1.0], 2, 1).len(), 2);
        // 空はそのまま空。
        assert!(resample_linear_mono(&[], 48_000, 16_000).is_empty());
    }

    #[test]
    fn mix_mono_sums_and_clamps() {
        // 加算してクランプ。長さは max。
        assert_eq!(mix_mono(&[0.5, 0.5], &[0.5, -0.5, 0.2]), vec![1.0, 0.0, 0.2]);
        // 片側空は他方そのまま。
        assert_eq!(mix_mono(&[0.3, 0.4], &[]), vec![0.3, 0.4]);
    }

    #[test]
    fn to_playback_mono_passthrough_and_transform() {
        // mono かつ同レート: 変換せず素通し（複製ゼロのパス＝値が一致）。
        assert_eq!(
            to_playback_mono(vec![0.1, 0.2, 0.3], 1, 48_000, 48_000),
            vec![0.1, 0.2, 0.3]
        );
        // 2ch は mono 化（平均）。同レートなので resample はしない。
        assert_eq!(
            to_playback_mono(vec![1.0, -1.0, 0.5, 0.5], 2, 48_000, 48_000),
            vec![0.0, 0.5]
        );
        // 異レートは resample（mono 入力・長さが変わる）。
        assert_eq!(to_playback_mono(vec![0.0, 1.0], 1, 1, 2).len(), 4);
    }
}
