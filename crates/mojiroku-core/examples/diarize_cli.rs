//! E2E / 開発用 CLI: 音声ファイル → 話者分離（誰がいつ話したか）。
//! 使い方: cargo run --release --example diarize_cli -- <audio> [models_dir]
//!
//! consolidation 込みの話者ターンを出力する。実会議で「話者がそれらしく分かれるか」を
//! 目視確認する検証ゲート（ADR-0009 の「動いた は実音声で確認」）。

fn main() {
    let audio = std::env::args()
        .nth(1)
        .expect("usage: diarize_cli <audio> [models_dir]");
    let models_dir = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/mojiroku-models".to_string());

    let on_progress = |stage: &str, done: u64, total: Option<u64>| {
        eprintln!("[{stage}] done={done} total={total:?}");
    };

    let t0 = std::time::Instant::now();
    let r = mojiroku_core::diarize_file(
        std::path::Path::new(&audio),
        std::path::Path::new(&models_dir),
        mojiroku_core::lang::Lang::Ja,
        Some(&on_progress),
    )
    .expect("diarize failed");
    let el = t0.elapsed().as_secs_f32();

    println!(
        "--- DIARIZATION: 話者数 {} / ターン {} （処理 {:.0}s）---",
        r.speakers.len(),
        r.turns.len(),
        el
    );
    for s in &r.speakers {
        println!("  speaker {} = {}", s.id, s.label);
    }
    // 話者別の合計尺
    use std::collections::BTreeMap;
    let mut dur: BTreeMap<&str, u64> = BTreeMap::new();
    for t in &r.turns {
        *dur.entry(t.speaker_id.as_str()).or_insert(0) += t.end_ms - t.start_ms;
    }
    print!("  尺(秒): ");
    for (spk, ms) in &dur {
        print!("{}={:.0} ", spk, *ms as f64 / 1000.0);
    }
    println!();
    println!("--- ターン ---");
    for t in &r.turns {
        println!(
            "  {:>7.2} -- {:>7.2}  {}",
            t.start_ms as f64 / 1000.0,
            t.end_ms as f64 / 1000.0,
            t.speaker_id
        );
    }
}
