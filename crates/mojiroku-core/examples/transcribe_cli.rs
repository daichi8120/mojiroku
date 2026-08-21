//! E2E / 開発用 CLI: 音声ファイル → 文字起こし。
//! 使い方: cargo run --example transcribe_cli -- <audio> [models_dir] [lang]
//!   lang: "ja"(既定) | "en" など whisper 言語コード | "auto"（言語自動判定）

fn main() {
    let audio = std::env::args()
        .nth(1)
        .expect("usage: transcribe_cli <audio> [models_dir] [lang]");
    let models_dir = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/mojiroku-models".to_string());
    let lang_arg = std::env::args().nth(3).unwrap_or_else(|| "ja".to_string());
    let language = match lang_arg.as_str() {
        "auto" => None,
        l => Some(l.to_string()),
    };

    let on_progress = |stage: &str, done: u64, total: Option<u64>| {
        eprintln!("[{stage}] done={done} total={total:?}");
    };

    let t = mojiroku_core::transcribe_file(
        std::path::Path::new(&audio),
        std::path::Path::new(&models_dir),
        language.as_deref(),
        Some(&on_progress),
    )
    .expect("transcribe failed");

    println!("--- TRANSCRIPT ({} segments) ---", t.segments.len());
    for s in &t.segments {
        println!("[{:>6}ms-{:>6}ms] {}", s.start_ms, s.end_ms, s.text);
    }
    // assert しやすいよう全文も 1 行で
    let full: String = t.segments.iter().map(|s| s.text.as_str()).collect();
    println!("FULLTEXT={full}");
}
