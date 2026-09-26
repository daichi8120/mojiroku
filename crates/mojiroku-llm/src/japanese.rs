//! Keep simplified-Chinese-only characters out of Japanese output (ADR-0043).
//!
//! Qwen models occasionally write a simplified form in the middle of Japanese text
//! (进 for 進, 报汇 for 報告). Prompting does not prevent it; on the 2026-09-23 gate,
//! Qwen3.5-4B did it in 3 of 28 outputs. The sampler therefore removes every vocabulary
//! token that contains such a character before sampling.

use llama_cpp_2::model::LlamaModel;
#[allow(deprecated)]
use llama_cpp_2::model::Special;
use llama_cpp_2::token::logit_bias::LlamaLogitBias;
use llama_cpp_2::token::LlamaToken;

/// Simplified forms that Windows-31J happens to encode but Japanese writing does not use.
const SIMPLIFIED_BUT_ENCODABLE: &[char] = &['个', '价'];

/// Whether `c` is a CJK ideograph outside the Japanese character set.
///
/// Windows-31J (JIS X 0208 plus the common vendor extensions) is the practical Japanese
/// set: kanji used in Japanese text encode, simplified-only forms do not. A few rare
/// Japanese kanji outside it (剝, 頰, 塡) have common alternatives (剥, 頬, 填).
pub fn is_non_japanese_ideograph(c: char) -> bool {
    // Every Han block: Extension A, the unified block, compatibility ideographs, and the
    // supplementary planes (Extensions B–J and the compatibility supplement; J is Unicode 17).
    // Compatibility forms Japanese uses, such as 﨑, are in Windows-31J and pass the check below.
    let cjk = matches!(
        c,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2FA1F}'
            | '\u{30000}'..='\u{3347F}'
    );
    if !cjk {
        return false;
    }
    if SIMPLIFIED_BUT_ENCODABLE.contains(&c) {
        return true;
    }
    let mut buf = [0u8; 4];
    let (_, _, unmappable) = encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buf));
    unmappable
}

/// A bias of negative infinity for each token whose text contains a non-Japanese ideograph.
///
/// Tokens that are not valid UTF-8 on their own (byte pieces of a longer character) are
/// left alone: they are shared with Japanese characters.
pub fn non_japanese_token_biases(model: &LlamaModel) -> Vec<LlamaLogitBias> {
    (0..model.n_vocab())
        .map(LlamaToken::new)
        .filter(|&token| {
            #[allow(deprecated)]
            let bytes = model.token_to_bytes(token, Special::Plaintext);
            bytes
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .is_some_and(|s| s.chars().any(is_non_japanese_ideograph))
        })
        .map(|token| LlamaLogitBias::new(token, f32::NEG_INFINITY))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplified_forms_seen_in_the_gate_are_blocked() {
        for c in ['户', '进', '报', '汇', '这', '们', '个'] {
            assert!(is_non_japanese_ideograph(c), "{c}");
        }
    }

    #[test]
    fn japanese_text_is_untouched() {
        let text =
            "議事録の決定事項を確認し、髙橋さんと﨑山さんが進捗を報告した。来週までに対応する";
        assert!(!text.chars().any(is_non_japanese_ideograph));
    }

    #[test]
    fn supplementary_and_compatibility_blocks_are_checked() {
        // Extension B (𠮷), Extension G (𰀀), and a compatibility ideograph outside
        // Windows-31J (U+F900 豈).
        for c in ['𠮷', '\u{30000}', '\u{F900}', '\u{323B0}', '\u{33479}'] {
            assert!(is_non_japanese_ideograph(c), "{c:?}");
        }
        // Compatibility ideographs that Windows-31J includes stay allowed.
        assert!(!is_non_japanese_ideograph('﨑'));
    }

    #[test]
    fn non_ideographs_are_untouched() {
        assert!(!"ToDo: 42 ✓ かなカナ".chars().any(is_non_japanese_ideograph));
    }
}
