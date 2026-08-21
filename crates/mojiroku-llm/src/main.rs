//! mojiroku ローカル要約 sidecar（llama.cpp / Metal）。
//!
//! whisper.cpp と llama.cpp は ggml シンボルが衝突するため、要約はこの別バイナリで実行する（ADR-0007）。
//!
//! プロトコル:
//!   引数1: GGUF モデルのパス（本体側が `mojiroku-core::models` で確保して渡す）
//!   引数2: プロンプトファイルのパス（ユーザー内容 = instruction + 文字起こし。
//!          `mojiroku-core::summarize::build_prompt` の出力を本体が temp に書いて渡す）
//!   引数3(任意): 生成最大トークン数（既定 2048）
//!   `--lang ja|en`(任意): システムプロンプト等の言語（既定 ja。位置引数のどこに置いてもよい）
//!   stdout: 生成された要約/議事録（プレーンテキスト）
//!   stderr: 進捗・診断ログ
//!
//! 本体（Tauri）は Tauri externalBin の sidecar として spawn する。
//! 巨大な文字起こしを引数で渡さないようファイル経由にしている。

use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
// Special は deprecated だが token_to_bytes（下記の据え置き理由参照）とセットで使う。
#[allow(deprecated)]
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;

// Qwen2.5-7B-Instruct のネイティブ context（旧 16_384 は半分で、90 分級の講義が
// 切り詰められていた）。要約は LLM 単独プロセスなので KV キャッシュ増（~1.8GB f16）は
// Apple Silicon で問題なし。実機メモリ逼迫時は 24_576 へ落とす。
const N_CTX: u32 = 32_768;

fn main() {
    // `--lang <ja|en>` を先に抜き取り、残りを位置引数として解釈する（既定 ja）。
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut lang = String::from("ja");
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        args.remove(i);
        if i < args.len() {
            lang = args.remove(i);
        }
    }
    let model_path = args
        .first()
        .cloned()
        .expect("usage: mojiroku-llm <model.gguf> <prompt_file> [max_tokens] [--lang ja|en]");
    let prompt_file = args.get(1).cloned().expect("prompt file path");
    let max_new: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2048);

    let user = std::fs::read_to_string(&prompt_file).expect("read prompt file");

    // Qwen2.5 ChatML を prefix / body(=文字起こし) / suffix に分けて扱う（既定モデルが Qwen 系）。
    // suffix の assistant 開始マーカーを「常に」残すのが肝。旧実装は ChatML 全体をトークン化
    // してから末尾を truncate していたため、長尺会議では suffix ごと欠落し、モデルが
    // 「文字起こしの続きを書く」エコー/反復暴走に陥っていた（崩壊出力の主因）。
    // システムプロンプトはコンテンツ言語（アプリ言語）に追従する。
    let prefix = if lang == "en" {
        "<|im_start|>system\nYou are a precise and concise meeting-minutes assistant.<|im_end|>\n<|im_start|>user\n"
    } else {
        "<|im_start|>system\nあなたは正確で簡潔な日本語の議事録アシスタントです。<|im_end|>\n<|im_start|>user\n"
    };
    let suffix = "<|im_end|>\n<|im_start|>assistant\n";

    let backend = LlamaBackend::init().expect("llama backend init");
    let model_params = LlamaModelParams::default().with_n_gpu_layers(1000); // Metal フルオフロード
    let model =
        LlamaModel::load_from_file(&backend, &model_path, &model_params).expect("load model");

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
    let mut ctx = model.new_context(&backend, ctx_params).expect("create ctx");

    // 1 回の decode が超えてはならない上限（context の n_batch 既定と一致）
    let n_batch: usize = 2048;

    // prefix/body/suffix を別々にトークン化し、body だけ予算内に収める。
    // Qwen2.5 は add_bos_token=false なので BOS は付けない（旧 AddBos::Always は実質 no-op）。
    let prefix_toks = model
        .str_to_token(prefix, AddBos::Never)
        .expect("tokenize prefix");
    let suffix_toks = model
        .str_to_token(suffix, AddBos::Never)
        .expect("tokenize suffix");
    let mut body_toks = model
        .str_to_token(user.trim(), AddBos::Never)
        .expect("tokenize body");

    // n_ctx を超えないよう、生成枠と ChatML エンベロープを残して body を切り詰める。
    let max_prompt = (N_CTX as usize).saturating_sub(max_new as usize + 16);
    let body_budget = max_prompt.saturating_sub(prefix_toks.len() + suffix_toks.len());
    if body_toks.len() > body_budget {
        // 講義/会議は結論・まとめ・Q&A が末尾に来るため、頭だけ残しは逆効果。
        // 予算の前 60% + 後 40% を残し、間を省略マーカーでつなぐ（頭尾保持）。
        eprintln!(
            "[mojiroku-llm] body {} tokens > {} -> 頭尾保持で切り詰め",
            body_toks.len(),
            body_budget
        );
        let ellipsis_text = if lang == "en" {
            "\n…(omitted)…\n"
        } else {
            "\n…（中略）…\n"
        };
        let ellipsis = model
            .str_to_token(ellipsis_text, AddBos::Never)
            .expect("tokenize ellipsis");
        let keep = body_budget.saturating_sub(ellipsis.len());
        let head_len = keep * 6 / 10;
        let tail_len = keep - head_len;
        let mut trimmed = Vec::with_capacity(body_budget);
        trimmed.extend_from_slice(&body_toks[..head_len]);
        trimmed.extend_from_slice(&ellipsis);
        trimmed.extend_from_slice(&body_toks[body_toks.len() - tail_len..]);
        body_toks = trimmed;
    }

    let mut tokens = prefix_toks;
    tokens.extend_from_slice(&body_toks);
    tokens.extend_from_slice(&suffix_toks); // assistant 開始マーカーは常に末尾に残る
    let prompt_len = tokens.len();
    eprintln!("[mojiroku-llm] prompt tokens = {prompt_len}");

    // プロンプトを n_batch ごとに分割して decode（GGML_ASSERT(n_tokens_all <= n_batch) 回避）
    let mut batch = LlamaBatch::new(n_batch, 1);
    let mut i = 0usize;
    while i < prompt_len {
        let end = (i + n_batch).min(prompt_len);
        batch.clear();
        for (j, &token) in tokens.iter().enumerate().take(end).skip(i) {
            let is_last = j == prompt_len - 1; // logits は最後のトークンだけ要求
            batch
                .add(token, j as i32, &[0], is_last)
                .expect("batch add");
        }
        ctx.decode(&mut batch).expect("decode prompt chunk");
        i = end;
    }

    let mut n_cur = prompt_len as i32;
    let n_max = n_cur + max_new;
    // 反復ブレーク用サンプラー（llama.cpp common_sampler 順: 履歴ペナルティ→切り詰め→温度→終端）。
    // 純グリーディは量子化 Qwen でループに陥りやすく Qwen 公式も非推奨。penalty_repeat は
    // presence-gate（1回適用）でグリーディ末尾だと高確信ループトークンを倒しきれないため、
    // 確率的 dist 末尾で確実にループ脱出する。penalty_freq=0 は日本語の頻出助詞(は/を/の)を
    // 不当に抑制しないため。固定 seed で再現性を確保（同一プロンプト→同一出力）。
    // 値は Qwen2.5 公式の生成推奨（temp=0.7 / top_p=0.8 / top_k=20 / rep≈1.05）に準拠。
    // ループ保険として rep を 1.1 へわずかに上げ（量子化対策）、nucleus を絞って
    // 指示追従（見出し書式など）の安定性を優先する。
    const SEED: u32 = 1234;
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(256, 1.1, 0.0, 0.0),
        LlamaSampler::top_k(20),
        LlamaSampler::top_p(0.8, 1),
        LlamaSampler::min_p(0.05, 1),
        LlamaSampler::temp(0.7),
        LlamaSampler::dist(SEED),
    ]);
    // マルチバイト UTF-8 がトークン境界で割れるのを防ぐため、バイト列を蓄積して最後にデコードする
    let mut out_bytes: Vec<u8> = Vec::new();

    while n_cur <= n_max {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        // token_to_bytes は deprecated（→ token_to_piece_bytes）だが、デコード挙動は
        // 要約品質に直結する（ChatML マーカー・反復ループ対策の履歴あり）ため、
        // 実モデルでの出力検証とセットで移行する。ここでは据え置きを明示する。
        #[allow(deprecated)]
        if let Ok(bytes) = model.token_to_bytes(token, Special::Tokenize) {
            out_bytes.extend_from_slice(&bytes);
        }
        batch.clear();
        batch.add(token, n_cur, &[0], true).expect("batch gen");
        n_cur += 1;
        ctx.decode(&mut batch).expect("decode gen");
    }

    let out = String::from_utf8_lossy(&out_bytes);
    print!("{}", out.trim());
}
