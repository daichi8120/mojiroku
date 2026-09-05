//! E2E / 開発用 CLI: 音声ファイル → 文字起こし。
//! 使い方: cargo run --example transcribe_cli -- <audio> [models_dir] [lang]
//!   lang: "ja"(既定) | "en" など whisper 言語コード | "auto"（言語自動判定）
//! Optional trailing arguments: [default|greedy|beam5] [--json] [--model <catalog-file>].
//! JSON mode emits one object on stdout; diagnostics stay on stderr.

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
    let decoder_arg = std::env::args().nth(4).unwrap_or_else(|| "default".into());
    use mojiroku_core::stt::{DecodingStrategy, FILE_DECODING};
    let decoding = match decoder_arg.as_str() {
        "default" => FILE_DECODING,
        "greedy" => DecodingStrategy::Greedy,
        "beam5" => DecodingStrategy::BeamSearch5,
        _ => panic!("decoder must be default, greedy, or beam5"),
    };
    let mut json_output = false;
    let mut model = mojiroku_core::models::select_whisper_model(None);
    let mut flags = std::env::args().skip(5);
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--json" => json_output = true,
            "--model" => {
                let file = flags.next().expect("--model needs a catalog file name");
                model = mojiroku_core::models::WHISPER_MODELS
                    .iter()
                    .find(|model| model.file == file)
                    .expect("unknown Whisper model; use a catalog file name");
            }
            _ => panic!("unknown option: {flag}"),
        }
    }

    let on_progress = |stage: &str, done: u64, total: Option<u64>| {
        eprintln!("[{stage}] done={done} total={total:?}");
    };

    let started = std::time::Instant::now();
    let t = mojiroku_core::transcribe_file_with_options(
        std::path::Path::new(&audio),
        std::path::Path::new(&models_dir),
        mojiroku_core::TranscriptionOptions {
            language: language.as_deref(),
            model,
            decoding,
        },
        Some(&on_progress),
    )
    .expect("transcribe failed");

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "transcript": t,
                "decoding": match decoding {
                    DecodingStrategy::Greedy => "greedy",
                    DecodingStrategy::BeamSearch5 => "beam5",
                },
                "pipeline_seconds": started.elapsed().as_secs_f64(),
                "whisper_model": model.file,
                "vad_model": mojiroku_core::models::DEFAULT_VAD_MODEL,
            })
        );
        return;
    }

    println!("--- TRANSCRIPT ({} segments) ---", t.segments.len());
    for s in &t.segments {
        println!("[{:>6}ms-{:>6}ms] {}", s.start_ms, s.end_ms, s.text);
    }
    // assert しやすいよう全文も 1 行で
    let full: String = t.segments.iter().map(|s| s.text.as_str()).collect();
    println!("FULLTEXT={full}");
}
