//! BYOK 要約（OpenAI / Anthropic）。`ureq` で blocking POST（core は非 async）。
//! ⚠️ BYOK 利用時はデータが端末外（各プロバイダ）へ送信される（プライバシーのトレードオフ）。

use super::{build_prompt, SummarizeProvider};
use crate::error::{CoreError, Result};
use crate::lang::Lang;
use crate::schemas::{Summary, SummaryTemplate, Transcript};

// システムプロンプトはコンテンツ言語に追従する（en はローカル sidecar mojiroku-llm と同文）。
const SYSTEM_PROMPT_JA: &str = "あなたは正確で簡潔な日本語の議事録アシスタントです。";
const SYSTEM_PROMPT_EN: &str = "You are a precise and concise meeting-minutes assistant.";

fn system_prompt(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => SYSTEM_PROMPT_JA,
        Lang::En => SYSTEM_PROMPT_EN,
    }
}

/// ureq エラーを人間可読に。Status(4xx/5xx) はレスポンスボディ（プロバイダの error.message。
/// 例: invalid x-api-key / model not found）も含める。これが無いと 401/404 の原因が分からない。
fn ureq_err(label: &str, e: ureq::Error) -> CoreError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: String = resp
                .into_string()
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect();
            CoreError::Model(format!("{label} {code}: {body}"))
        }
        other => CoreError::Model(format!("{label} request: {other}")),
    }
}

/// HTTP 200 でも content が取れない（想定外形/空）場合は成功扱いにせずエラーにする。
/// 空要約を黙って保存する「静かな失敗」を防ぐ。
fn require_content(label: &str, content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Err(CoreError::Model(format!(
            "{label}: 応答が空でした（モデル名やレスポンス形式を確認してください）"
        )));
    }
    Ok(())
}

/// OpenAI Chat Completions（gpt-4o-mini 等）。
pub struct OpenAiSummarizer {
    pub api_key: String,
    pub model: String,
    /// 出力のコンテンツ言語（システムプロンプトと build_prompt のマーカーに反映）。
    pub lang: Lang,
}

impl SummarizeProvider for OpenAiSummarizer {
    fn summarize(&self, transcript: &Transcript, template: &SummaryTemplate) -> Result<Summary> {
        let user = build_prompt(transcript, template, self.lang);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt(self.lang)},
                {"role": "user", "content": user},
            ],
        });
        let resp = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(body)
            .map_err(|e| ureq_err("openai", e))?;
        let v: serde_json::Value = resp
            .into_json()
            .map_err(|e| CoreError::Model(format!("openai json: {e}")))?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        require_content("openai", &content)?;
        Ok(Summary {
            template_id: template.id.clone(),
            content,
            action_items: Vec::new(),
            stale: false,
        })
    }
}

/// Anthropic Messages（claude-* 等）。
pub struct AnthropicSummarizer {
    pub api_key: String,
    pub model: String,
    /// 出力のコンテンツ言語（システムプロンプトと build_prompt のマーカーに反映）。
    pub lang: Lang,
}

impl SummarizeProvider for AnthropicSummarizer {
    fn summarize(&self, transcript: &Transcript, template: &SummaryTemplate) -> Result<Summary> {
        let user = build_prompt(transcript, template, self.lang);
        let body = serde_json::json!({
            // 議事録は長くなり得るため余裕を持たせる（1024 だと日本語議事録が途中で切れる）。
            "model": self.model,
            "max_tokens": 4096,
            "system": system_prompt(self.lang),
            "messages": [
                {"role": "user", "content": user},
            ],
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .send_json(body)
            .map_err(|e| ureq_err("anthropic", e))?;
        let v: serde_json::Value = resp
            .into_json()
            .map_err(|e| CoreError::Model(format!("anthropic json: {e}")))?;
        let content = v["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        require_content("anthropic", &content)?;
        Ok(Summary {
            template_id: template.id.clone(),
            content,
            action_items: Vec::new(),
            stale: false,
        })
    }
}
