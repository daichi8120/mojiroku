//! データモデル。話者分離対応（`Segment.speaker_id`）を最初から内包する（`docs/03_design/spec.md` §5）。
//!
//! Phase 1 では `speaker_id` を未割当（`None`）で埋め、Phase 2 の話者分離導入時に実値を付与する。
//! これによりスキーマの retrofit が不要になる。

use serde::{Deserialize, Serialize};

/// 入力ソース種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    File,
    Mic,
    Live,
}

/// 要約テンプレートの種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    Minutes,
    Summary,
    ActionItems,
}

/// 録音 / セッション（ルートエンティティ）。履歴・エクスポートはここに紐づく。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub source_type: SourceType,
    pub title: Option<String>,
    pub duration_ms: u64,
    pub sample_rate: u32,
    /// RFC3339 形式のタイムスタンプ
    pub created_at: String,
}

/// 文字起こし結果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

/// セグメント。`speaker_id` を Phase 1 から保持し、話者分離の retrofit を不要にする。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker_id: Option<String>,
}

/// 話者
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    pub id: String,
    pub label: String,
    pub display_name: Option<String>,
}

/// 要約 / 議事録（生成された出力）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub template_id: String,
    pub content: String,
    pub action_items: Vec<ActionItem>,
    /// 元の文字起こし/話者が後から更新され、この要約が古くなったか（後付け話者分離で立つ・ADR-0024）。
    /// 生成直後は常に false。読み戻し（`get_recording_detail`）でのみ DB の値が入る。旧クライアント/
    /// 旧データとの互換のため serde default。
    #[serde(default)]
    pub stale: bool,
}

/// アクションアイテム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub text: String,
    pub assignee: Option<String>,
    pub due: Option<String>,
}

/// 要約テンプレート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryTemplate {
    pub id: String,
    pub name: String,
    pub kind: TemplateKind,
    pub prompt: String,
}
