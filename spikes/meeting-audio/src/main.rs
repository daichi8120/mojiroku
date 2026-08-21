//! Phase 7 会議モード スパイク — macOS のシステム音声を ScreenCaptureKit でキャプチャできるか、
//! そして「未署名(ad-hoc) .app + TCC」で実機が成立するかを **計測** するための throwaway 実験。
//!
//! 検証したいこと（ADR-0017 のための一次データ）:
//!   1. `screencapturekit` (Rust binding v8) が macOS 26.5 でビルド・動作するか（音声経路の marshaling）
//!   2. SCShareableContent::get() で TCC ダイアログが出るか／許可後にコールバックが発火するか
//!   3. PCM が **非無音** で流れるか（RMS > 0）／実フォーマット（48k stereo か要望どおりか）
//!   4. （.app バンドル化後）どの Privacy ペインに載るか・再ビルドで許可が失効/サイレント拒否するか
//!
//! 使い方: `meeting-audio-spike [秒数] [出力WAVパス]`（既定: 20秒 / ~/Desktop/mojiroku-spike-capture.wav）
//! 期待動作: 何か音を鳴らしている状態（音楽/通話）で起動 → ログに非ゼロ RMS、WAV に相手音声が録れる。

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use screencapturekit::prelude::*;

/// `package.sh` が毎回書き換えるビルドタグ。include_str! で cargo の rerun 依存になり、
/// 値が変わると必ず再コンパイル → バイナリ(cdhash)が変わる。再ビルド→TCC失効サイクルの計測用。
const BUILD_TAG: &str = include_str!("build_tag.txt");

/// little-endian な生バイト列を f32 サンプル列へ再解釈（端数は捨てる）。
fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// `open MojirokuSpike.app` で起動すると stdio が切り離されるため、計測ログは
/// ファイルにも残す。stderr とログファイルの両方へ1行追記する。
fn flog(path: &str, msg: &str) {
    eprintln!("{msg}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let out_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| format!("{home}/Desktop/mojiroku-spike-capture.wav"));
    let log_path = format!("{home}/Desktop/mojiroku-spike-log.txt");
    // 毎回まっさらから（前回ログを残さない）。
    let _ = std::fs::write(&log_path, "");

    flog(&log_path, "== mojiroku Phase 7 system-audio spike ==");
    flog(&log_path, &format!("build_tag={} (cdhash 識別用)", BUILD_TAG.trim()));
    flog(&log_path, &format!("launched. capture={secs}s out={out_path}"));
    flog(&log_path, "[1] SCShareableContent::get() を呼びます（ここで TCC『画面/システム音声』ダイアログが出るはず）...");

    // ここが TCC のトリガ点。未署名/ad-hoc .app で許可ダイアログが出るか／許可が通るかを観察する。
    let content = match SCShareableContent::get() {
        Ok(c) => c,
        Err(e) => {
            flog(&log_path, &format!("[!] SCShareableContent::get() 失敗: {e:?}"));
            flog(&log_path, "    → 権限未許可か未対応の可能性。System Settings > Privacy & Security >");
            flog(&log_path, "      Screen & System Audio Recording を確認して再実行してください。");
            return Err(Box::new(e));
        }
    };
    let displays = content.displays();
    let display = displays.first().ok_or("no display found")?;
    flog(&log_path, &format!("[2] OK: displays={} 個。先頭ディスプレイのシステム音声をキャプチャします。", displays.len()));

    // 表示全体のシステム音声を取得するフィルタ（ウィンドウ除外なし）。
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // 音声専用のつもりだが SCK は映像を完全に切れない → 2x2 ダミー映像にして無視する。
    // 16k/mono を要望しても native(48k/stereo f32) で来る可能性が高い（実測で確認する）。
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_sample_rate(48_000_i32)
        .with_channel_count(2_i32)
        .with_excludes_current_process_audio(true);

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let audio_cbs = Arc::new(AtomicU64::new(0));
    let video_cbs = Arc::new(AtomicU64::new(0));
    let peak_rms = Arc::new(Mutex::new(0.0_f32));

    let mut stream = SCStream::new(&filter, &config);

    // 映像（Screen）は no-op だが、known-good パターンに合わせてハンドラを登録する
    // （音声のみで start するとコールバックが来ない実装差異を避ける）。
    {
        let video_cbs = Arc::clone(&video_cbs);
        stream.add_output_handler(
            move |_sample: CMSampleBuffer, of_type: SCStreamOutputType| {
                if matches!(of_type, SCStreamOutputType::Screen) {
                    video_cbs.fetch_add(1, Ordering::Relaxed);
                }
            },
            SCStreamOutputType::Screen,
        );
    }

    // 音声（Audio）: AudioBufferList を mono にダウンミックスして蓄積、RMS を計測。
    {
        let samples = Arc::clone(&samples);
        let audio_cbs = Arc::clone(&audio_cbs);
        let peak_rms = Arc::clone(&peak_rms);
        stream.add_output_handler(
            move |sample: CMSampleBuffer, of_type: SCStreamOutputType| {
                if !matches!(of_type, SCStreamOutputType::Audio) {
                    return;
                }
                let n = audio_cbs.fetch_add(1, Ordering::Relaxed);
                let Some(list) = sample.audio_buffer_list() else {
                    if n < 3 {
                        eprintln!("[audio] cb#{n}: audio_buffer_list() = None");
                    }
                    return;
                };

                // 各 buffer を f32 化。planar(buffer=channel) と interleaved の両対応。
                let bufs: Vec<(u32, Vec<f32>)> = list
                    .iter()
                    .map(|b| (b.number_channels, bytes_to_f32(b.data())))
                    .collect();

                let mono: Vec<f32> = if bufs.len() > 1 {
                    // planar: 各 buffer が 1ch。フレーム長は最小に合わせて平均。
                    let frames = bufs.iter().map(|(_, d)| d.len()).min().unwrap_or(0);
                    (0..frames)
                        .map(|i| {
                            let sum: f32 = bufs.iter().map(|(_, d)| d[i]).sum();
                            sum / bufs.len() as f32
                        })
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
                };

                // RMS（非無音判定）
                let rms = if mono.is_empty() {
                    0.0
                } else {
                    (mono.iter().map(|x| x * x).sum::<f32>() / mono.len() as f32).sqrt()
                };
                if let Ok(mut pr) = peak_rms.lock() {
                    if rms > *pr {
                        *pr = rms;
                    }
                }

                if n < 5 || n % 50 == 0 {
                    eprintln!(
                        "[audio] cb#{n}: buffers={} ch0={} frames(mono)={} rms={:.5}",
                        bufs.len(),
                        bufs.first().map(|(c, _)| *c).unwrap_or(0),
                        mono.len(),
                        rms
                    );
                }

                if let Ok(mut s) = samples.lock() {
                    s.extend_from_slice(&mono);
                }
            },
            SCStreamOutputType::Audio,
        );
    }

    flog(&log_path, &format!("[3] start_capture() — {secs} 秒キャプチャ。音楽/通話など音を鳴らしてください..."));
    let t0 = Instant::now();
    stream.start_capture()?;
    std::thread::sleep(std::time::Duration::from_secs(secs));
    stream.stop_capture()?;
    let elapsed = t0.elapsed().as_secs_f64();

    let mono = samples.lock().unwrap().clone();
    let audio_n = audio_cbs.load(Ordering::Relaxed);
    let video_n = video_cbs.load(Ordering::Relaxed);
    let pr = *peak_rms.lock().unwrap();
    // WAV は「要求した 48000」で書く（実測レート推定で書くと latency/疎なコールバックで
    // ピッチがずれ『壊れている』と誤読される）。実測比は診断としてのみログする。
    const REQUESTED_RATE: u32 = 48_000;
    let frames_per_sec = if elapsed > 0.0 { mono.len() as f64 / elapsed } else { 0.0 };

    flog(&log_path, "\n== 計測結果 ==");
    flog(&log_path, &format!("audio callbacks : {audio_n}"));
    flog(&log_path, &format!("video callbacks : {video_n}"));
    flog(&log_path, &format!("mono frames     : {}", mono.len()));
    flog(&log_path, &format!("elapsed         : {elapsed:.2}s"));
    flog(&log_path, &format!("requested rate  : {REQUESTED_RATE} Hz（WAV はこのレートで書く）"));
    flog(&log_path, &format!("frames/elapsed  : {frames_per_sec:.0} /s（連続音なら ~48000 に近いはず。大きくズレるなら sound-gated か別レート）"));
    flog(&log_path, &format!("peak RMS        : {pr:.5}  → {}", if pr > 1e-4 { "非無音 OK（音が録れている）" } else { "ほぼ無音 ⚠️（音を鳴らしていたか/権限を確認）" }));

    if mono.is_empty() {
        flog(&log_path, "[!] サンプルが空。コールバック未発火＝TCC 拒否/未対応の可能性。WAV は書き出しません。");
        return Ok(());
    }

    // 要求レート(48000)で mono WAV を書き出し（耳で確認できる成果物）。
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: REQUESTED_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&out_path, spec)?;
    for &x in &mono {
        let v = (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        w.write_sample(v)?;
    }
    w.finalize()?;
    flog(&log_path, &format!("[4] WAV 書き出し: {out_path}"));
    flog(&log_path, &format!("    → afplay '{out_path}' で再生して、相手側の音が録れているか確認してください。"));
    Ok(())
}
