//! Notion / Slack エクスポータで共有する pure な Markdown ヘルパー。
//!
//! ⚠️ `parse_heading` は**共有しない**。notion は見出し内に残った `#` を保持し
//! （`## # 議題` → `(2, "# 議題")`）、slack は全 `#` を剥がす（`## # 議題` → `"議題"`）。
//! これは「重複」ではなく**意図的なプラットフォーム差**なので、各モジュールに据え置く。
//! `is_thematic_break` / `chunk_chars` も slack 固有（notion は `---` を落とさず、
//! rich_text は JSON 生成にチャンク幅を埋め込む）のため共有対象にしない。

use crate::lang::Lang;

/// テンプレ id → 出力見出しラベル。
/// ⚠️ frontend `templates.ts` の `TEMPLATE_LABELS`/`FALLBACK_LABEL` と**両言語とも完全一致必須**
/// （ズレると Notion/Slack 追記と Obsidian/印刷の見出しの整合が壊れる。両側のテストで固定）。
pub(crate) fn template_label(id: &str, lang: Lang) -> &'static str {
    match (lang, id) {
        (Lang::Ja, "minutes") => "議事録",
        (Lang::Ja, "summary") => "要約",
        (Lang::Ja, "action_items") => "アクションアイテム",
        (Lang::Ja, _) => "メモ",
        (Lang::En, "minutes") => "Minutes",
        (Lang::En, "summary") => "Summary",
        (Lang::En, "action_items") => "Action Items",
        (Lang::En, _) => "Notes",
    }
}

/// `- ` / `* ` / `+ ` を箇条書きテキストに。
pub(crate) fn parse_bullet(line: &str) -> Option<&str> {
    for p in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(p) {
            return Some(rest.trim());
        }
    }
    None
}

/// 先頭 `limit` 文字に丸める（char 単位、マルチバイト安全）。
pub(crate) fn cap_chars(s: &str, limit: usize) -> String {
    s.chars().take(limit).collect()
}

/// 会議タイトルを解決する。前後空白を落とし、空/未設定なら既定の「会議」/ "Meeting"。
/// 返り値は入力（または 'static の既定値）を借用する。
pub(crate) fn meeting_title(raw: Option<&str>, lang: Lang) -> &str {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(match lang {
            Lang::Ja => "会議",
            Lang::En => "Meeting",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ 期待値は frontend templates.test.ts と突き合わせて**両言語ともハードコード**で固定する
    /// （ja: 議事録/要約/アクションアイテム/メモ、en: Minutes/Summary/Action Items/Notes）。
    #[test]
    fn template_label_maps_known_and_falls_back() {
        assert_eq!(template_label("minutes", Lang::Ja), "議事録");
        assert_eq!(template_label("summary", Lang::Ja), "要約");
        assert_eq!(template_label("action_items", Lang::Ja), "アクションアイテム");
        assert_eq!(template_label("unknown", Lang::Ja), "メモ");
        assert_eq!(template_label("", Lang::Ja), "メモ");
        assert_eq!(template_label("minutes", Lang::En), "Minutes");
        assert_eq!(template_label("summary", Lang::En), "Summary");
        assert_eq!(template_label("action_items", Lang::En), "Action Items");
        assert_eq!(template_label("unknown", Lang::En), "Notes");
        assert_eq!(template_label("", Lang::En), "Notes");
    }

    #[test]
    fn meeting_title_trims_and_falls_back_per_lang() {
        assert_eq!(meeting_title(Some("  定例  "), Lang::Ja), "定例");
        assert_eq!(meeting_title(Some("   "), Lang::Ja), "会議");
        assert_eq!(meeting_title(None, Lang::Ja), "会議");
        assert_eq!(meeting_title(None, Lang::En), "Meeting");
        assert_eq!(meeting_title(Some(""), Lang::En), "Meeting");
    }

    #[test]
    fn parse_bullet_accepts_three_markers_and_trims() {
        assert_eq!(parse_bullet("- 箇条"), Some("箇条"));
        assert_eq!(parse_bullet("*  星  "), Some("星"));
        assert_eq!(parse_bullet("+ プラス"), Some("プラス"));
        // プレフィックス直後にスペースが無いものは箇条書き扱いしない。
        assert_eq!(parse_bullet("-ダッシュ"), None);
        assert_eq!(parse_bullet("本文"), None);
    }

    #[test]
    fn cap_chars_is_multibyte_safe() {
        // char 単位で丸める（マルチバイトをバイト境界で割らない）。
        assert_eq!(cap_chars("あいうえお", 3), "あいう");
        assert_eq!(cap_chars("あいうえお", 2), "あい");
        assert_eq!(cap_chars("あいうえお", 10), "あいうえお");
        assert_eq!(cap_chars("", 5), "");
        assert_eq!(cap_chars("abc", 0), "");
    }
}
