//! マイク録音（cpal）。`Stream` は `!Send` のため専用ワーカースレッドに閉じ込め、
//! managed state には停止シグナルと共有バッファだけを持つ。録音は OS/CoreAudio 依存なので
//! `mojiroku-core`（Tauri/UI 非依存）ではなく `src-tauri` 側に置く。
//!
//! spool 化（ADR-0023）: PCM は全量 RAM に貯めず、ワーカーが定期的に spool WAV へ
//! 追記して共有バッファの先頭を解放する。停止時は WAV を finalize してパスを返し、
//! 呼び出し側（commands/recording.rs）が正式名へ rename する。

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

use crate::audio::spool::{SharedPcm, WavSpoolWriter};

/// spool への書き出し周期。クラッシュ時に失われるのは最大この間隔ぶん。
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// 共有バッファに残す末尾の長さ（秒）。live_stt の未確定 tail（最大 14s）+ tick +
/// 重処理中の長い whisper 実行（数十秒）を吸収する余裕を持たせる。
const KEEP_TAIL_SECS: usize = 30;

/// 録音中セッション。`Stream` はワーカースレッド内のみに生き、ここには持たない。
pub struct RecordingSession {
    stop_tx: Sender<()>,
    samples: Arc<SharedPcm>,
    /// ワーカーが finalize 後に (spool_path, samples_written) を返す。
    result_rx: Receiver<Result<(PathBuf, u64), String>>,
    handle: JoinHandle<()>,
    sample_rate: u32,
    channels: u16,
    /// flush 中の IO エラー（disk full 等）。チャンクは破棄して録音を継続した印。
    spool_error: Arc<Mutex<Option<String>>>,
}

/// 録音セッションの managed state（同時に 1 つ）。
pub struct MicState(pub Mutex<Option<RecordingSession>>);

impl MicState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// `stop()` の結果。PCM 本体は spool WAV にある（メモリで返さない）。
pub struct MicStopInfo {
    pub spool_path: PathBuf,
    /// 書き込んだサンプル総数（interleaved）。0 = 無録音。
    pub samples_written: u64,
    pub sample_rate: u32,
    pub channels: u16,
    /// flush 中の IO エラー（部分保存で継続した警告。None = 完全）。
    pub spool_error: Option<String>,
}

fn normalize_i16(s: i16) -> f32 {
    s as f32 / 32768.0
}

fn normalize_u16(s: u16) -> f32 {
    (s as f32 - 32768.0) / 32768.0
}

/// 録音開始。default 入力デバイスで stream を build し、別スレッドで回しつつ
/// `spool_path` の WAV へ逐次書き出す。`Stream` の build/play/drop はすべてスレッド内で
/// 完結させる（`!Send` 回避）。spool WAV の作成失敗はここで即 Err。
pub fn start(state: &MicState, spool_path: PathBuf) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    if guard.is_some() {
        return Err("error.mic.busy".into());
    }

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("error.mic.no_input_device")?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("error.mic.input_config: {e}"))?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let mut spool = WavSpoolWriter::create(&spool_path, sample_rate, channels)
        .map_err(|e| format!("spool create: {e}"))?;

    let samples = Arc::new(SharedPcm::new());
    let samples_cb = Arc::clone(&samples);
    let shared = Arc::clone(&samples);
    let spool_error = Arc::new(Mutex::new(None));
    let spool_error_w = Arc::clone(&spool_error);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<(PathBuf, u64), String>>();
    let keep_tail = KEEP_TAIL_SECS * sample_rate as usize * channels as usize;

    // device/config/Stream は !Send を含むのでスレッドへ move して中で build する。
    let handle = std::thread::spawn(move || {
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &_| samples_cb.push(data),
                |err| eprintln!("cpal stream error: {err}"),
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &_| {
                    let f: Vec<f32> = data.iter().map(|&s| normalize_i16(s)).collect();
                    samples_cb.push(&f);
                },
                |err| eprintln!("cpal stream error: {err}"),
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _: &_| {
                    let f: Vec<f32> = data.iter().map(|&s| normalize_u16(s)).collect();
                    samples_cb.push(&f);
                },
                |err| eprintln!("cpal stream error: {err}"),
                None,
            ),
            other => {
                eprintln!("未対応のサンプル形式: {other:?}");
                // 失敗しても空 spool を finalize して stop() に 0 サンプルを返す
                //（従来の「空 PCM → error.recording.empty」挙動を保つ）。
                let _ = result_tx.send(spool.finalize());
                return;
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("build_input_stream 失敗: {e}");
                let _ = result_tx.send(spool.finalize());
                return;
            }
        };
        if let Err(e) = stream.play() {
            eprintln!("play 失敗: {e}");
            drop(stream);
            let _ = result_tx.send(spool.finalize());
            return;
        }

        // flush ループ: FLUSH_INTERVAL ごとに末尾 KEEP_TAIL を残して spool へ追記。
        // IO エラー時はチャンクを破棄して録音は継続する（部分保存 > 全損）。
        loop {
            match stop_rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let chunk = shared.take_flush_chunk(keep_tail);
                    if !chunk.is_empty() {
                        if let Err(e) = spool.append(&chunk).and_then(|_| spool.flush()) {
                            *spool_error_w.lock().unwrap() = Some(e);
                        }
                    }
                }
            }
        }
        drop(stream); // CoreAudio capture を同期停止 → 以後 push は来ない

        // 最終 drain → finalize。
        let rest = shared.take_flush_chunk(0);
        if !rest.is_empty() {
            if let Err(e) = spool.append(&rest) {
                *spool_error_w.lock().unwrap() = Some(e);
            }
        }
        let _ = result_tx.send(spool.finalize());
    });

    *guard = Some(RecordingSession {
        stop_tx,
        samples,
        result_rx,
        handle,
        sample_rate,
        channels,
        spool_error,
    });
    Ok(())
}

/// 録音停止。spool WAV を finalize し、パス・サンプル総数・native rate/channels を返す
/// （rename・文字起こしは呼び出し側）。join 後に受けるのでコールバックとの競合なし。
pub fn stop(state: &MicState) -> Result<MicStopInfo, String> {
    let session = state.0.lock().unwrap().take().ok_or("録音していません")?;
    let RecordingSession {
        stop_tx,
        samples: _,
        result_rx,
        handle,
        sample_rate,
        channels,
        spool_error,
    } = session;
    let _ = stop_tx.send(());
    let _ = handle.join();
    let (spool_path, samples_written) = result_rx
        .recv()
        .map_err(|_| "録音ワーカーが結果を返しませんでした".to_string())??;
    let spool_error = spool_error.lock().unwrap().take();
    Ok(MicStopInfo {
        spool_path,
        samples_written,
        sample_rate,
        channels,
        spool_error,
    })
}

/// ライブ文字起こし（増分C）用に、進行中録音の共有バッファ・native rate・channels を返す。
/// 録音中でなければ None。バッファは spool flush で先頭が解放されるため、読者は
/// `SharedPcm::snapshot_from` の絶対 index で追従する（`live_stt::take_new`）。
pub fn live_handle(state: &MicState) -> Option<(Arc<SharedPcm>, u32, u16)> {
    let guard = state.0.lock().unwrap();
    guard
        .as_ref()
        .map(|s| (Arc::clone(&s.samples), s.sample_rate, s.channels))
}

/// インターリーブ済みサンプル数・チャンネル・レートから録音長(ms)。
pub fn duration_ms(sample_count: usize, channels: u16, sample_rate: u32) -> u64 {
    if sample_rate == 0 || channels == 0 {
        return 0;
    }
    let frames = sample_count as u64 / channels as u64;
    frames * 1000 / sample_rate as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_normalization() {
        assert!((normalize_i16(i16::MAX) - 0.99997).abs() < 1e-3);
        assert_eq!(normalize_i16(0), 0.0);
        assert_eq!(normalize_i16(i16::MIN), -1.0);
    }

    #[test]
    fn u16_normalization() {
        assert!(normalize_u16(0) + 1.0 < 1e-3); // 0 → ~-1.0
        assert_eq!(normalize_u16(32768), 0.0);
    }

    #[test]
    fn duration_calc() {
        assert_eq!(duration_ms(96_000, 2, 48_000), 1000); // 48k/2ch/96000 = 1000ms
        assert_eq!(duration_ms(16_000, 1, 16_000), 1000);
        assert_eq!(duration_ms(0, 2, 48_000), 0);
        assert_eq!(duration_ms(100, 0, 48_000), 0);
    }
}
