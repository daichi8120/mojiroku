//! Dev CLI: run the Silero VAD alone on one audio file and print the speech spans it returns,
//! with per-span RMS, so VAD parameters can be evaluated without running whisper
//! (Issue #65, missing-text half).
//!
//! Usage: cargo run --release --example vad_spans_cli -- <audio> <models_dir> [threshold] [min_speech_ms] [min_silence_ms]
//!   defaults are whisper.cpp's: threshold 0.5, min_speech 250, min_silence 100

use std::path::PathBuf;

use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let audio = args.get(1).expect(
        "usage: vad_spans_cli <audio> <models_dir> [threshold] [min_speech_ms] [min_silence_ms]",
    );
    let models_dir = PathBuf::from(args.get(2).expect("models_dir"));
    let threshold: f32 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0.5);
    let min_speech: i32 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(250);
    let min_silence: i32 = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(100);

    let pcm = mojiroku_core::audio::decode_to_pcm16k_mono(audio).expect("decode");
    let total_ms = pcm.len() as u64 * 1000 / 16_000;

    let vad = models_dir.join(mojiroku_core::models::DEFAULT_VAD_MODEL);
    // Same rule as the product STT path (ADR-0021): whisper.cpp calls go through the FFI guard so
    // a C++ exception (e.g. bad_alloc) becomes an error message instead of a process abort.
    let segs = mojiroku_core::ffi_guard::guard("silero vad", || {
        let mut ctx =
            WhisperVadContext::new(&vad.to_string_lossy(), WhisperVadContextParams::new())?;
        let mut p = WhisperVadParams::new();
        p.set_threshold(threshold);
        p.set_min_speech_duration(min_speech);
        p.set_min_silence_duration(min_silence);
        ctx.segments_from_samples(p, &pcm)
    })
    .expect("vad (foreign exception)")
    .expect("vad");

    let mut kept_ms = 0u64;
    let mut n = 0usize;
    println!("# threshold={threshold} min_speech={min_speech} min_silence={min_silence} total={total_ms}ms");
    println!("#   start_ms    end_ms   dur_ms  rms_db");
    for seg in segs {
        let s_ms = (seg.start.max(0.0) * 10.0) as u64;
        let e_ms = (seg.end.max(0.0) * 10.0) as u64;
        let i0 = (s_ms * 16) as usize;
        let i1 = ((e_ms * 16) as usize).min(pcm.len());
        let slice = &pcm[i0..i1.max(i0)];
        let rms = if slice.is_empty() {
            0.0
        } else {
            (slice.iter().map(|x| x * x).sum::<f32>() / slice.len() as f32).sqrt()
        };
        let db = 20.0 * (rms.max(1e-9)).log10();
        println!("{s_ms:>10} {e_ms:>9} {:>8} {db:>7.1}", e_ms - s_ms);
        kept_ms += e_ms - s_ms;
        n += 1;
    }
    println!(
        "# spans={n} kept={kept_ms}ms ({:.1}% of audio)",
        kept_ms as f64 * 100.0 / total_ms as f64
    );
}
