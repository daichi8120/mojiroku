//! 外部サービスへの書き出し（連携）。送信はユーザー操作起点のみ。
//!
//! 現状: Notion（内部インテグレーション トークン = BYOK, 要約+文字起こし）/
//! Slack（Incoming Webhook = BYOK, 要約のみ）。将来: PDF 等。
//! トークン/webhook URL 等のシークレットは平文に置かずキーチェーン管轄（src-tauri の `secrets`）。

mod common;
pub mod notion;
pub mod slack;

pub use notion::{accessible_pages as notion_accessible_pages, NotionExporter, NotionPage};
pub use slack::SlackExporter;
