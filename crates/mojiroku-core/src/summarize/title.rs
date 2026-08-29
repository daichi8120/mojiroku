//! 録音タイトルの自動生成（Issue #4 増分2）。
//!
//! **位置づけは「既定タイトルの改善」であって、識別の代替ではない。**
//! 誰と会ったかは音声に出てこない（会議で目の前の相手の名前は呼ばない）。カレンダーの
//! 予定名がある録音ではそちらを使い、無いときだけ「何を話したか」で `録音` / `会議` を置き換える。
//! 実会議 19 本での測定は Issue #4 のコメントにある。
//!
//! ここはプロンプトの組み立てと出力の後処理だけを持つ。実行はローカル sidecar（別プロセス）
//! と BYOK で経路が違うので、**両方がこの同じ関数を通る**ようにして経路依存部を最小にする。

use crate::lang::Lang;
use crate::schemas::{SummaryTemplate, TemplateKind, Transcript};

/// タイトルとして受け入れる最大文字数。指示は 20〜30 字を狙うが、超えたぶんを切り詰めると
/// 意味の壊れた断片が残るので、**切らずに捨てて既定タイトルへ倒す**。
const MAX_TITLE_CHARS: usize = 40;

/// ⚠️ この文面は実データ 19 本で 3 版試した結果（Issue #4）。変更するなら測り直すこと。
/// 効いた指示: 固有名詞を「強制」せず「自信がなければ使うな」に緩める（聞き取り誤りの人名が
/// 題名に昇格するのを防ぐ）／日付を明示的に禁じる（`recording.rs` の「タイトルにタイムスタンプを
/// 埋めない」方針と、Qwen が日付を創作する既知の罠の両方に効く）。
const TITLE_INSTRUCTION_JA: &str = "以下の会議の文字起こしに、日本語で短いタイトルを1つだけ付けてください。あとで履歴一覧を眺めたときに、タイトルだけでどの会議だったか思い出せることが目的です。何について話した集まりなのかが一目で分かる言葉を選んでください。会社名・製品名・講義名・イベント名が文字起こしにはっきり出ていれば入れると手がかりになりますが、聞き取りが怪しい語や自信のない固有名詞は使わないでください。日付・曜日・時刻は書かないでください。20文字程度、長くても30文字までにしてください。日本語として自然な言い回しにしてください。文字起こしに無い情報は創作しないでください。前置き・説明・引用符・記号は書かず、タイトルの本文だけを1行で出力してください。";

const TITLE_INSTRUCTION_EN: &str = "Give the meeting transcript below a single short title in English. The goal is that when scanning the history list later, the title alone is enough to remember which meeting this was. Choose words that show at a glance what the meeting was about. If a company, product, course or event name appears clearly in the transcript, including it helps as a cue - but do not use names you are unsure about or that look mis-transcribed. Do not write dates, weekdays or times. Keep it around 40 characters and no longer than 60. Do not invent information that is not in the transcript. Output only the title itself on one line, with no preamble, explanation or quotation marks.";

/// タイトル生成用の擬似テンプレート。[`super::build_prompt`] に渡して、要約と同じ
/// 区切りマーカー・同じ文字起こし整形を共有する（本文の作り方を二重に持たない）。
///
/// `builtin_templates` には**入れない**。ユーザーが要約テンプレートとして選ぶものではなく、
/// 内部利用だから（一覧に出ると「タイトル」を要約として実行できてしまう）。
fn title_template(lang: Lang) -> SummaryTemplate {
    let (name, prompt) = match lang {
        Lang::Ja => ("タイトル", TITLE_INSTRUCTION_JA),
        Lang::En => ("Title", TITLE_INSTRUCTION_EN),
    };
    SummaryTemplate {
        id: "title".to_string(),
        name: name.to_string(),
        kind: TemplateKind::Summary,
        prompt: prompt.to_string(),
    }
}

/// 文字起こし → タイトル生成のプロンプト。長すぎる本文の切り詰めは sidecar 側が
/// 頭尾保持で行う（`crates/mojiroku-llm`）ので、ここでは全文を渡す。
pub fn build_title_prompt(transcript: &Transcript, lang: Lang) -> String {
    super::build_prompt(transcript, &title_template(lang), lang)
}

/// LLM の生出力 → タイトルとして使える 1 行。使えなければ `None`（呼び出し側は既定
/// タイトルのままにする＝**生成に失敗しても録音の保存は成功する**）。
///
/// 実出力で確認した壊れ方に対応する（Issue #4 の測定）。
/// - 推論モデルの `<think>…</think>`（思考が途中で切れて答えに届かない場合も含む）
/// - 2 行目以降に説明が続く
/// - 前後の引用符・かぎ括弧、`タイトル:` のような前置き
///
/// **中国語・英語の混入は検出しない。** 語としては自然に見えるので機械的に弾けず、
/// モデル選択の問題として Issue #4 に既知の限界として記録してある。
pub fn sanitize_title(raw: &str) -> Option<String> {
    let body = strip_thinking(raw)?;

    let line = body.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = strip_label(line);
    let line = strip_wrappers(line);
    let line = line.trim().trim_end_matches(['。', '.']).trim();

    if line.is_empty() || line.chars().count() > MAX_TITLE_CHARS {
        return None;
    }
    Some(line.to_string())
}

/// 推論モデルの思考ブロックを落とす。閉じタグが無い＝思考の途中で生成が尽きていて
/// 答えが存在しないので、`None`（切り詰めた思考をタイトルにしない）。
fn strip_thinking(raw: &str) -> Option<&str> {
    match raw.find("<think>") {
        None => Some(raw),
        Some(_) => raw.split("</think>").nth(1),
    }
}

/// `タイトル:` `Title:` のような前置きを落とす。1 行目の先頭にしか現れない。
fn strip_label(line: &str) -> &str {
    for label in ["タイトル:", "タイトル：", "Title:", "title:"] {
        if let Some(rest) = line.strip_prefix(label) {
            return rest.trim();
        }
    }
    line
}

/// 前後を包む引用符・かぎ括弧を落とす（対になっているときだけ）。
fn strip_wrappers(line: &str) -> &str {
    let mut s = line;
    for (open, close) in [
        ('「', '」'),
        ('『', '』'),
        ('【', '】'),
        ('"', '"'),
        ('\'', '\''),
        ('“', '”'),
    ] {
        if s.starts_with(open) && s.ends_with(close) && s.chars().count() >= 2 {
            s = s
                .strip_prefix(open)
                .and_then(|r| r.strip_suffix(close))
                .unwrap_or(s)
                .trim();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 実測の出力をそのまま固定する（Issue #4・2026-08-24 の 10 本から） ──

    #[test]
    fn accepts_real_outputs_as_is() {
        // 綺麗に 1 行で返ってきたもの。そのまま通す。
        assert_eq!(
            sanitize_title("インターンシップ面談"),
            Some("インターンシップ面談".to_string())
        );
        assert_eq!(
            sanitize_title("作業スケジュール自動生成システム"),
            Some("作業スケジュール自動生成システム".to_string())
        );
    }

    #[test]
    fn keeps_language_defects_by_design() {
        // 中国語・英語の混入は**検出しない**。語として自然に見えるので機械的に弾けない。
        // モデル選択の問題として扱う（Issue #4）。ここで落とすと「会議」に戻るだけで、
        // 利用者にとって改善にならない。
        assert_eq!(
            sanitize_title("LLM評価軸探讨"),
            Some("LLM評価軸探讨".to_string())
        );
        assert_eq!(
            sanitize_title("論文進捗 discuss"),
            Some("論文進捗 discuss".to_string())
        );
    }

    #[test]
    fn takes_first_line_when_explanation_follows() {
        assert_eq!(
            sanitize_title("開発会議\nこの会議では次期リリースについて話し合われました。"),
            Some("開発会議".to_string())
        );
    }

    #[test]
    fn strips_thinking_block() {
        // 推論モデル（Qwen3 系）。思考を抜けた先の答えを取る。
        assert_eq!(
            sanitize_title("<think>\nWe need a short title.\n</think>\n\nインターン面談"),
            Some("インターン面談".to_string())
        );
    }

    #[test]
    fn rejects_unterminated_thinking() {
        // 思考の途中で max_tokens が尽きた場合。答えが存在しないので既定タイトルへ倒す。
        // 実測: Qwen3-Swallow-8B-SFT は 512 トークンでも思考が終わらなかった。
        assert_eq!(
            sanitize_title("<think>\nWe need to produce a short Japanese title (20-30"),
            None
        );
    }

    #[test]
    fn strips_wrappers_and_labels() {
        assert_eq!(sanitize_title("「開発定例」"), Some("開発定例".to_string()));
        assert_eq!(sanitize_title("\"開発定例\""), Some("開発定例".to_string()));
        assert_eq!(
            sanitize_title("タイトル: 開発定例"),
            Some("開発定例".to_string())
        );
        assert_eq!(
            sanitize_title("タイトル：「開発定例」"),
            Some("開発定例".to_string())
        );
        // 句点は落とす（タイトルに文末記号は要らない）。
        assert_eq!(sanitize_title("開発定例。"), Some("開発定例".to_string()));
    }

    #[test]
    fn rejects_empty_and_overlong() {
        assert_eq!(sanitize_title(""), None);
        assert_eq!(sanitize_title("   \n  "), None);
        assert_eq!(sanitize_title("「」"), None);
        // 40 字ちょうどは通し、41 字は捨てる。切り詰めると意味の壊れた断片が残るので捨てる。
        let forty = "あ".repeat(40);
        assert_eq!(sanitize_title(&forty), Some(forty.clone()));
        assert_eq!(sanitize_title(&"あ".repeat(41)), None);
    }

    #[test]
    fn prompt_contains_instruction_and_transcript() {
        use crate::schemas::Segment;
        let t = Transcript {
            language: Some("ja".into()),
            segments: vec![Segment {
                idx: 0,
                start_ms: 0,
                end_ms: 1000,
                text: "予算の話をしました".into(),
                speaker_id: None,
            }],
        };
        let p = build_title_prompt(&t, Lang::Ja);
        assert!(p.contains("短いタイトルを1つだけ"), "指示が入っていない");
        assert!(p.contains("予算の話をしました"), "文字起こしが入っていない");
        // タイトルは要約テンプレート一覧に出さない（利用者が選ぶものではない）。
        assert!(
            !super::super::builtin_templates(Lang::Ja)
                .iter()
                .any(|t| t.id == "title"),
            "title が builtin_templates に混ざっている"
        );
    }
}
