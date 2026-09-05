//! Dev CLI: transcribe one audio file twice, with and without the Silero VAD, and print both
//! transcripts so the two can be diffed. Used to measure whether VAD drops real speech
//! (Issue #65, missing-text half).
//!
//! Usage: cargo run --release --example vad_ab_cli -- <audio> <models_dir> [lang] [out_dir]
//!   lang: "ja" (default) | "en" | "auto"
//!   out_dir: where `<stem>.vad-on.txt` / `<stem>.vad-off.txt` are written (default: cwd)

use std::path::{Path, PathBuf};

use mojiroku_core::stt::{SttEngine, WhisperStt};

fn write_transcript(path: &Path, t: &mojiroku_core::schemas::Transcript) {
    let mut s = String::new();
    for seg in &t.segments {
        s.push_str(&format!(
            "[{:>7}ms-{:>7}ms] {}\n",
            seg.start_ms, seg.end_ms, seg.text
        ));
    }
    std::fs::write(path, s).expect("write transcript");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let audio = args
        .get(1)
        .expect("usage: vad_ab_cli <audio> <models_dir> [lang] [out_dir]");
    let models_dir = PathBuf::from(args.get(2).expect("models_dir"));
    let language = match args.get(3).map(String::as_str).unwrap_or("ja") {
        "auto" => None,
        l => Some(l.to_string()),
    };
    let out_dir = PathBuf::from(args.get(4).map(String::as_str).unwrap_or("."));
    let stem = Path::new(audio)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());

    let pcm = mojiroku_core::audio::decode_to_pcm16k_mono(audio).expect("decode");
    eprintln!(
        "decoded {} samples ({:.1}s)",
        pcm.len(),
        pcm.len() as f32 / 16_000.0
    );

    let whisper = models_dir.join(mojiroku_core::models::DEFAULT_WHISPER_MODEL);
    let vad = models_dir.join(mojiroku_core::models::DEFAULT_VAD_MODEL);
    assert!(whisper.exists(), "missing {}", whisper.display());
    assert!(vad.exists(), "missing {}", vad.display());

    for (label, vad_path) in [("vad-on", Some(vad.clone())), ("vad-off", None)] {
        let t0 = std::time::Instant::now();
        let engine = WhisperStt::load(&whisper, vad_path).expect("load whisper");
        let t = engine
            .transcribe(&pcm, language.as_deref())
            .expect("transcribe");
        let chars: usize = t.segments.iter().map(|s| s.text.chars().count()).sum();
        let speech_ms: u64 = t
            .segments
            .iter()
            .map(|s| s.end_ms.saturating_sub(s.start_ms))
            .sum();
        eprintln!(
            "{label}: {} segments, {chars} chars, {speech_ms} ms covered, {:.1}s wall",
            t.segments.len(),
            t0.elapsed().as_secs_f32()
        );
        write_transcript(&out_dir.join(format!("{stem}.{label}.txt")), &t);
    }
}
