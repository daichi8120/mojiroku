//! 非機密のアプリ設定（要約エンジン選択・プロバイダ・プライバシートグル）の永続化。
//!
//! 保存先は `app_data_dir/settings.json`（DB やモデルと同じ親）。`std::fs` で直接書くため
//! JS の fs プラグイン/capability は不要。シークレット（API キー）は **ここには入れず**
//! キーチェーン（[`crate::secrets`]）に保管する。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// settings.json のファイル名。
const FILE: &str = "settings.json";

/// 要約エンジンのプロバイダ既定モデル（`model` が空のときに使う）。
/// ⚠️ モデル名は将来変わり得るうえ、利用可否は各ユーザーのアカウント次第。
/// UI で編集可能にしてあり、これは「未指定時の出発点」に過ぎない。
/// claude-3-5-sonnet 系は 2025-10-28 に提供終了（API 404）のため現行 4 系を既定にする。
const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";

fn default_engine() -> String {
    "local".into()
}
fn default_provider() -> String {
    "anthropic".into()
}
fn default_true() -> bool {
    true
}

/// 永続化するアプリ設定。フィールドが欠けた古い settings.json でも安全に既定へ倒れるよう
/// per-field `serde(default)` を付ける。シークレットは含めない（キーチェーン管轄）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// "local"（同梱モデル, 既定） | "cloud"（BYOK）。
    #[serde(default = "default_engine")]
    pub engine: String,
    /// "anthropic" | "openai"（cloud のとき有効）。
    #[serde(default = "default_provider")]
    pub provider: String,
    /// モデル名の上書き。空なら provider 既定（[`Settings::effective_model`]）。
    #[serde(default)]
    pub model: String,
    /// 録音原本を保存するか（既定 ON）。
    #[serde(default = "default_true")]
    pub save_recordings: bool,
    /// 匿名の使用状況送信（既定 OFF）。
    #[serde(default)]
    pub send_usage: bool,
    /// Notion 連携の親ページ ID または URL（空なら未設定）。
    /// トークンはここに置かずキーチェーン（[`crate::secrets`]）管轄。
    #[serde(default)]
    pub notion_parent_id: String,
    /// アプリ言語 "ja" | "en"。UI 表示のほか、要約の出力言語・話者ラベル・
    /// エクスポート見出しにも使う（コンテンツ言語を兼ねる）。
    /// 空 = 未設定（初回起動でフロントが OS 言語から解決して保存する）。
    #[serde(default)]
    pub language: String,
    /// 文字起こし（whisper）の言語 "auto" | "ja" | "en"。
    /// 空 = 既定（アプリ言語に追従）。"auto" は whisper の言語自動判定。
    #[serde(default)]
    pub transcribe_language: String,
    /// 会議開始時に録音を促す通知を出すか（ADR-0026・増分1）。既定 OFF＝明示オプトイン。
    /// カレンダー連携が前提（未接続時はスケジューラが何もしない）。
    #[serde(default)]
    pub auto_record_prompt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: default_engine(),
            provider: default_provider(),
            model: String::new(),
            save_recordings: true,
            send_usage: false,
            notion_parent_id: String::new(),
            language: String::new(),
            transcribe_language: String::new(),
            auto_record_prompt: false,
        }
    }
}

impl Settings {
    /// API へ送る実モデル名。`model` 未指定なら provider 既定へ解決する
    /// （空文字を API に送ると 400 になるため、ここで必ず非空にする）。
    pub fn effective_model(&self) -> String {
        let m = self.model.trim();
        if !m.is_empty() {
            return m.to_string();
        }
        match self.provider.as_str() {
            "openai" => OPENAI_DEFAULT_MODEL,
            _ => ANTHROPIC_DEFAULT_MODEL,
        }
        .to_string()
    }

    /// アプリ言語（コンテンツ言語を兼ねる）。未設定（旧 settings.json / 初回起動前）は
    /// 従来挙動の "ja" に倒す。
    pub fn effective_language(&self) -> &str {
        if self.language == "en" {
            "en"
        } else {
            "ja"
        }
    }

    /// 文字起こし（whisper）へ渡す言語。`None` = whisper の自動判定。
    /// 空（既定）はアプリ言語に追従し、未設定の旧 settings.json では従来どおり "ja"。
    pub fn effective_transcribe_language(&self) -> Option<&str> {
        match self.transcribe_language.as_str() {
            "auto" => None,
            "ja" | "en" => Some(self.transcribe_language.as_str()),
            _ => Some(self.effective_language()),
        }
    }
}

/// settings.json を読む。無い/壊れているなら既定。
pub fn load(data_dir: &Path) -> Settings {
    match std::fs::read(data_dir.join(FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// settings.json を原子的に書く（temp → rename。半端な JSON を残さない）。
pub fn save(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    let path = data_dir.join(FILE);
    let json = serde_json::to_vec_pretty(settings).map_err(|e| format!("settings encode: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| format!("settings write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("settings rename: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 言語フィールドの無い旧 settings.json（v0.4.0 以前）は language="" に倒れ、
    /// effective_* が従来挙動（日本語）を返すこと（後方互換）。
    #[test]
    fn old_settings_json_defaults_to_ja() {
        let s: Settings =
            serde_json::from_str(r#"{"engine":"local","provider":"anthropic"}"#).unwrap();
        assert_eq!(s.language, "");
        assert_eq!(s.transcribe_language, "");
        assert_eq!(s.effective_language(), "ja");
        assert_eq!(s.effective_transcribe_language(), Some("ja"));
    }

    /// transcribe_language が空のときはアプリ言語に追従する。
    #[test]
    fn transcribe_language_follows_app_language() {
        let s = Settings {
            language: "en".into(),
            ..Settings::default()
        };
        assert_eq!(s.effective_language(), "en");
        assert_eq!(s.effective_transcribe_language(), Some("en"));
    }

    /// "auto" は whisper の自動判定（None）。明示指定はアプリ言語より優先される。
    #[test]
    fn transcribe_language_auto_and_explicit() {
        let mut s = Settings {
            language: "en".into(),
            transcribe_language: "auto".into(),
            ..Settings::default()
        };
        assert_eq!(s.effective_transcribe_language(), None);
        s.transcribe_language = "ja".into();
        assert_eq!(s.effective_transcribe_language(), Some("ja"));
    }

    /// 未知の language 値は "ja" に倒す（設定ファイルの手編集耐性）。
    #[test]
    fn unknown_language_falls_back_to_ja() {
        let s = Settings {
            language: "fr".into(),
            ..Settings::default()
        };
        assert_eq!(s.effective_language(), "ja");
    }
}
