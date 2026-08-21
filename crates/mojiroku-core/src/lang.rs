//! コンテンツ言語（要約の出力言語・話者ラベル・エクスポート見出しの ja/en）。
//!
//! アプリ設定 `language`（src-tauri の settings）に追従する。文字起こし（whisper）へ渡す
//! 言語ヒント（`Option<&str>`、None = 自動判定）とは**別物**なので混同しないこと
//! ── 例: 文字起こしは "auto" でもコンテンツ言語は ja、という組み合わせがあり得る。

/// コンテンツ言語。未設定・未知の値は従来挙動の日本語に倒す（[`Lang::from_code`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Ja,
    En,
}

impl Lang {
    /// 言語コード → `Lang`。"en" のみ En、それ以外（"ja"・空・未知の値）はすべて Ja
    /// （設定ファイルの手編集や旧 settings.json への耐性）。
    pub fn from_code(s: &str) -> Self {
        if s == "en" {
            Lang::En
        } else {
            Lang::Ja
        }
    }

    /// `Lang` → 言語コード（要約 sidecar の `--lang` 等へ渡す）。
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }
}

/// 既定話者ラベル（ja「話者N」空白なし / en「Speaker N」空白あり）。ラベルは生成時の言語で
/// DB に固定される（diarization の `SherpaDiarizer` 参照）。フロントの `speakerLabelFromId` と
/// 表記を一致させること。`n` は表示子 ── 話者分離は連番 `usize`、要約は "S1" から抜いた桁
/// 文字列 `&str`（"007" の桁を保存）を渡す。
pub fn default_speaker_label<N: std::fmt::Display>(n: N, lang: Lang) -> String {
    match lang {
        Lang::Ja => format!("話者{n}"),
        Lang::En => format!("Speaker {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_code_maps_en_and_falls_back_to_ja() {
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("ja"), Lang::Ja);
        // 未知・空は従来挙動の ja に倒す。
        assert_eq!(Lang::from_code("fr"), Lang::Ja);
        assert_eq!(Lang::from_code(""), Lang::Ja);
    }

    #[test]
    fn code_roundtrips() {
        assert_eq!(Lang::Ja.code(), "ja");
        assert_eq!(Lang::En.code(), "en");
        assert_eq!(Lang::from_code(Lang::En.code()), Lang::En);
        // 既定は ja（旧 settings.json の挙動保存）。
        assert_eq!(Lang::default(), Lang::Ja);
    }

    #[test]
    fn default_speaker_label_matches_diar_and_summary_forms() {
        // 話者分離: 連番 usize。ja は空白なし・en は空白あり。
        assert_eq!(default_speaker_label(1usize, Lang::Ja), "話者1");
        assert_eq!(default_speaker_label(1usize, Lang::En), "Speaker 1");
        // 要約: "S007" から抜いた桁 &str は桁を保存する。
        assert_eq!(default_speaker_label("007", Lang::Ja), "話者007");
        assert_eq!(default_speaker_label("007", Lang::En), "Speaker 007");
    }
}
