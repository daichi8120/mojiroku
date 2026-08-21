//! E2E / 開発用 CLI: 音声ファイル → 文字起こし＋話者分離（話者付き Transcript）。
//! 使い方: cargo run --release --example transcribe_diarize_cli -- <audio> [models_dir]
//!
//! whisper(STT) → sherpa(diarization) を 1 プロセスで走らせる＝同居の実走確認も兼ねる
//! （whisper が 0 セグメント化しないこと。ADR-0009 の最終確認）。

fn main() {
    let audio = std::env::args()
        .nth(1)
        .expect("usage: transcribe_diarize_cli <audio> [models_dir]");
    let models_dir = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/mojiroku-models".to_string());

    let on_progress = |stage: &str, done: u64, total: Option<u64>| {
        eprintln!("[{stage}] done={done} total={total:?}");
    };

    let (transcript, speakers, _embeddings) = mojiroku_core::transcribe_and_diarize_file(
        std::path::Path::new(&audio),
        std::path::Path::new(&models_dir),
        Some("ja"),
        mojiroku_core::lang::Lang::Ja,
        Some(&on_progress),
    )
    .expect("transcribe+diarize failed");

    println!(
        "--- 同居実走 OK: STT {} セグメント / 話者 {} ---",
        transcript.segments.len(),
        speakers.len()
    );
    assert!(
        !transcript.segments.is_empty(),
        "whisper が 0 セグメント化（同居で壊れた可能性）"
    );
    for s in &speakers {
        println!("  {} = {}", s.id, s.label);
    }
    println!("--- 話者付きセグメント ---");
    for s in &transcript.segments {
        println!(
            "  [{:>7.2}-{:>7.2}] {:<3} {}",
            s.start_ms as f64 / 1000.0,
            s.end_ms as f64 / 1000.0,
            s.speaker_id.as_deref().unwrap_or("?"),
            s.text.trim()
        );
    }
}
