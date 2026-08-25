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
//!   `--no-think`(任意): 思考を出さずに本文から書かせる（Qwen3 系の推論モデル向け）
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
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;

// Qwen2.5-7B-Instruct のネイティブ context（旧 16_384 は半分で、90 分級の講義が
// 切り詰められていた）。要約は LLM 単独プロセスなので KV キャッシュ増（~1.8GB f16）は
// Apple Silicon で問題なし。実機メモリ逼迫時は 24_576 へ落とす。
const N_CTX: u32 = 32_768;

/// システムプロンプト。コンテンツ言語（アプリ言語）に追従する。
fn system_prompt(lang: &str) -> &'static str {
    if lang == "en" {
        "You are a precise and concise meeting-minutes assistant."
    } else {
        "あなたは正確で簡潔な日本語の議事録アシスタントです。"
    }
}

/// Qwen2.5 の ChatML を手で組む。**モデルが chat template を持たないときだけ使う**
/// フォールバック。GGUF にテンプレートが入っていない古い変換への保険。
fn chatml_fallback(lang: &str, body: &str, no_think: bool) -> String {
    let think = if no_think { "<think>\n\n</think>\n\n" } else { "" };
    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n{}",
        system_prompt(lang),
        body,
        think
    )
}

/// プロンプト文字列を組む。**モデル自身の chat template を最優先で使う。**
///
/// なぜモデル任せにするか: 以前は Qwen2.5 の ChatML を固定で組んでいた。既定モデルが
/// Qwen 系だったので気づかなかったが、他系統のモデルには native でないテンプレートを
/// 食わせていたことになる。gemma / Llama / Phi は `<|im_end|>` を特殊トークンとして持たない
/// ため、それを**ただの文字列として出力**し、`is_eog_token` に一度も引っかからず生成が
/// 止まらなかった（2026-08-25 実測: 非 Qwen 系 5 本すべてで漏れ、Qwen 系 6 本は 0 件）。
///
/// llama.cpp の `llama_chat_apply_template` は GGUF に焼かれたテンプレートを使うので、
/// モデルを差し替えても正しいマーカーが付く。トークナイズ側は `parse_special = true` なので、
/// テンプレート由来の特殊トークンは文字列ではなく特殊トークンとして解釈される。
fn render_prompt(
    model: &LlamaModel,
    tmpl: Option<&LlamaChatTemplate>,
    lang: &str,
    body: &str,
    no_think: bool,
) -> String {
    let Some(t) = tmpl else {
        return chatml_fallback(lang, body, no_think);
    };
    let sys = system_prompt(lang);

    // system ロールを受け付けないモデルがある（gemma 系）。弾かれたら system を user の
    // 冒頭に畳んで組み直す。捨てるのではなく畳むのは、指示の言語を保つため。
    let attempts: [Vec<(&str, String)>; 2] = [
        vec![("system", sys.to_string()), ("user", body.to_string())],
        vec![("user", format!("{sys}\n\n{body}"))],
    ];
    for msgs in attempts {
        let built: Result<Vec<_>, _> = msgs
            .into_iter()
            .map(|(role, content)| LlamaChatMessage::new(role.to_string(), content))
            .collect();
        let Ok(built) = built else { continue };
        if let Ok(mut s) = model.apply_chat_template(t, &built, true) {
            if no_think {
                // Qwen3 系の思考を切る。テンプレートは add_ass=true で assistant の
                // 開始まで出しているので、その直後に空の think ブロックを置けばよい。
                s.push_str("<think>\n\n</think>\n\n");
            }
            return s;
        }
    }
    eprintln!("[mojiroku-llm] chat template の適用に失敗 -> ChatML へフォールバック");
    chatml_fallback(lang, body, no_think)
}

/// 組み上がったプロンプト文字列をトークン化する。
///
/// **BOS を付けるかはテンプレート次第**なので、決め打ちしない。llama.cpp のテンプレート
/// エンジンは、BOS を文字列として出すもの（Qwen 系は そもそも BOS 不要）と、出さずに
/// トークナイズ側で付ける前提のもの（gemma 系）が混在する。
/// 先に BOS 無しで引いてみて、先頭が BOS でなければモデルの設定に従って付け直す。
///
/// 実測: これを `AddBos::Never` に固定していたとき、gemma は BOS 無しのプロンプトを受け取り
/// 「---」しか返さなかった。テンプレート自体は正しく当たっていたので、原因が見えにくい。
fn tokenize_prompt(model: &LlamaModel, prompt: &str) -> Vec<llama_cpp_2::token::LlamaToken> {
    let plain = model
        .str_to_token(prompt, AddBos::Never)
        .expect("tokenize prompt");
    if plain.first() == Some(&model.token_bos()) {
        return plain;
    }
    // AddBos::Always は「モデルの add_bos_token 設定に従う」の意味。BOS を要らない
    // モデル（Qwen 系）では何も足されないので、付け過ぎにはならない。
    model
        .str_to_token(prompt, AddBos::Always)
        .unwrap_or(plain)
}

/// トークン列を文字列へ戻す（切り詰めた本文をテンプレートへ入れ直すため）。
fn detokenize(model: &LlamaModel, tokens: &[llama_cpp_2::token::LlamaToken]) -> String {
    let mut bytes = Vec::new();
    for &t in tokens {
        #[allow(deprecated)]
        if let Ok(b) = model.token_to_bytes(t, Special::Tokenize) {
            bytes.extend_from_slice(&b);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

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
    // `--no-think`: 思考を出させない（Qwen3 系の推論モデル向け）。既定 false＝従来挙動。
    let no_think = args.iter().position(|a| a == "--no-think").map(|i| {
        args.remove(i);
        true
    }) == Some(true);
    let model_path = args.first().cloned().expect(
        "usage: mojiroku-llm <model.gguf> <prompt_file> [max_tokens] [--lang ja|en] [--no-think]",
    );
    let prompt_file = args.get(1).cloned().expect("prompt file path");
    let max_new: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2048);

    let user = std::fs::read_to_string(&prompt_file).expect("read prompt file");

    let backend = LlamaBackend::init().expect("llama backend init");
    let model_params = LlamaModelParams::default().with_n_gpu_layers(1000); // Metal フルオフロード
    let model =
        LlamaModel::load_from_file(&backend, &model_path, &model_params).expect("load model");

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
    let mut ctx = model.new_context(&backend, ctx_params).expect("create ctx");

    // 1 回の decode が超えてはならない上限（context の n_batch 既定と一致）
    let n_batch: usize = 2048;

    // モデル自身の chat template を取る。無ければ render_prompt が ChatML へ落ちる。
    let tmpl = model.chat_template(None).ok();
    if tmpl.is_none() {
        eprintln!("[mojiroku-llm] chat template を持たないモデル -> ChatML で組む");
    }

    // テンプレートの長さはモデルごとに違うので、**空の本文で一度組んで実測**する
    // （prefix/suffix を決め打ちしていた頃の前提はもう無い）。
    let max_prompt = (N_CTX as usize).saturating_sub(max_new as usize + 16);
    let overhead = tokenize_prompt(
        &model,
        &render_prompt(&model, tmpl.as_ref(), &lang, "", no_think),
    )
    .len();
    let body_toks = model
        .str_to_token(user.trim(), AddBos::Never)
        .expect("tokenize body");
    let mut body_budget = max_prompt.saturating_sub(overhead);

    // 講義/会議は結論・まとめ・Q&A が末尾に来るため、頭だけ残しは逆効果。
    // 予算の前 60% + 後 40% を残し、間を省略マーカーでつなぐ（頭尾保持）。
    //
    // 本文をトークン列で切ってから文字列へ戻し、テンプレートに入れ直す。戻して入れ直すと
    // トークン数がわずかに変わり得るので、収まるまで予算を詰めて組み直す（最大 3 回）。
    let ellipsis_text = if lang == "en" {
        "\n…(omitted)…\n"
    } else {
        "\n…（中略）…\n"
    };
    let ellipsis_len = model
        .str_to_token(ellipsis_text, AddBos::Never)
        .map(|t| t.len())
        .unwrap_or(8);

    let mut tokens;
    let mut attempt = 0;
    loop {
        let body = if body_toks.len() > body_budget {
            let keep = body_budget.saturating_sub(ellipsis_len);
            let head_len = keep * 6 / 10;
            let tail_len = keep - head_len;
            format!(
                "{}{}{}",
                detokenize(&model, &body_toks[..head_len]),
                ellipsis_text,
                detokenize(&model, &body_toks[body_toks.len() - tail_len..])
            )
        } else {
            user.trim().to_string()
        };
        let prompt = render_prompt(&model, tmpl.as_ref(), &lang, &body, no_think);
        tokens = tokenize_prompt(&model, &prompt);
        attempt += 1;
        if tokens.len() <= max_prompt || attempt >= 3 {
            break;
        }
        eprintln!(
            "[mojiroku-llm] prompt {} tokens > {} -> 予算を詰めて組み直す（{attempt} 回目）",
            tokens.len(),
            max_prompt
        );
        // 必ず切り詰めが効くよう、本文長より小さい値まで落としてから 1 割詰める。
        body_budget = body_budget.min(body_toks.len()) * 9 / 10;
    }
    let prompt_len = tokens.len();
    eprintln!("[mojiroku-llm] prompt tokens = {prompt_len} (overhead {overhead})");
    // モデルを差し替えたとき、テンプレートが期待どおり当たっているかを確かめる口。
    // 出力がおかしいときは、まずここで実際に渡している文字列を見る。
    if std::env::var("MOJIROKU_LLM_DEBUG").is_ok() {
        let dump = render_prompt(&model, tmpl.as_ref(), &lang, "《本文》", no_think);
        eprintln!("[mojiroku-llm] --- 組み上がったプロンプト（本文は伏字） ---\n{dump}\n--- ここまで ---");
    }

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
