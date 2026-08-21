//! Notion へ議事録を書き出す（内部インテグレーション トークン = BYOK, $0, OAuth 不要）。
//!
//! ⚠️ 送信時にデータ（要約 + 文字起こし）が Notion のサーバへ出る（プライバシーのトレードオフ）。
//! 呼び出しはユーザー操作（ボタン）起点のみ。トークンは平文に置かずキーチェーン管轄
//! （[`crate::store`] ではなく src-tauri の `secrets`）。
//!
//! ## 構成
//! - 本モジュール（`notion.rs`）: **Notion HTTP/API クライアント**。ページ作成/追記
//!   （`NotionExporter`）、書き出し先候補の列挙（`accessible_pages`）、ID 正規化・エラー整形。
//! - `blocks` サブモジュール: **議事録 → Notion ブロック JSON への変換**（純粋層・I/O 無し）。
//!
//! API バージョンは安定版 `2022-06-28` をピン。`page_id` 親 + paragraph/heading/bulleted_list_item +
//! 応答の `url` フィールドは 2021 年から不変のコア。⚠️ もし database 親に変えるなら、新しい
//! Notion-Version は `database_id` ではなく `data_source_id` を要求する → ここの前提が崩れる
//! （MVP を **ページ親**で完結させている理由）。

use super::common::{cap_chars, meeting_title};
use crate::error::{CoreError, Result};
use crate::lang::Lang;
use crate::store::RecordingDetail;
use serde_json::{json, Value};

/// 議事録 → Notion ブロック JSON への変換（純粋層・ネットワーク I/O 無し）。
mod blocks;
use blocks::build_blocks;

/// 安定版にピン（page 親 + 基本ブロックはこのバージョンで不変）。
const NOTION_VERSION: &str = "2022-06-28";
const API_PAGES: &str = "https://api.notion.com/v1/pages";
/// Notion: ページ作成 children / append children は 1 リクエスト 100 ブロックまで。
const MAX_CHILDREN: usize = 100;
/// Notion: rich_text 1 要素は 2000 文字まで（超えると 400 validation_error）。
/// 応答本文の title cap（本モジュール）と blocks の rich_text チャンク（`super::RICH_TEXT_LIMIT`
/// で参照）で共有する Notion API 制約。
const RICH_TEXT_LIMIT: usize = 2000;
/// Notion API 呼び出しの全体タイムアウト（秒）。応答ストールでスレッドが無期限ブロックしない保険。
const HTTP_TIMEOUT_SECS: u64 = 60;

/// Notion へ議事録ページを作る BYOK エクスポータ。
pub struct NotionExporter {
    /// 内部インテグレーション トークン（`ntn_...` / `secret_...`）。キーチェーン由来。
    pub token: String,
    /// 親ページの ID または Notion ページ URL（[`normalize_page_id`] が吸収する）。
    pub parent_id: String,
    /// 見出し・既定文言のコンテンツ言語（エクスポート実行時のアプリ設定に追従）。
    pub lang: Lang,
}

impl NotionExporter {
    /// 親ページ配下に議事録ページを作成し、作成された Notion ページの URL を返す。
    pub fn export(&self, detail: &RecordingDetail) -> Result<String> {
        let parent = normalize_page_id(&self.parent_id)?;
        let title = meeting_title(detail.recording.title.as_deref(), self.lang);
        let blocks = build_blocks(detail, self.lang);

        // 応答ストール（read 段の無期限ブロック）でスレッドが永久に返らないよう全体タイムアウトを設ける。
        // ureq 既定は connect 30s のみで read/write は無期限。
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build();

        // 1) ページ作成（最初の 100 ブロックまで同梱。残りは後で append）。
        let head_len = blocks.len().min(MAX_CHILDREN);
        let body = json!({
            "parent": { "type": "page_id", "page_id": parent },
            "properties": {
                "title": { "title": [ { "type": "text", "text": { "content": cap_chars(title, RICH_TEXT_LIMIT) } } ] }
            },
            "children": &blocks[..head_len],
        });
        let resp = agent
            .post(API_PAGES)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Notion-Version", NOTION_VERSION)
            .send_json(body)
            .map_err(|e| notion_err("notion ページ作成", e))?;
        let v: Value = resp
            .into_json()
            .map_err(|e| CoreError::Model(format!("notion json: {e}")))?;
        let page_id = v["id"]
            .as_str()
            .ok_or_else(|| CoreError::Model("notion: 応答に id がありません".to_string()))?
            .to_string();
        let url = v["url"].as_str().unwrap_or_default().to_string();

        // 2) 残りを 100 ずつ追記（無言の切り捨てはしない）。
        for chunk in blocks[head_len..].chunks(MAX_CHILDREN) {
            self.append(&agent, &page_id, chunk)?;
        }
        Ok(url)
    }

    /// 既存ページ末尾へブロックを追記（1 回 100 まで）。agent はタイムアウト設定を export と共有。
    fn append(&self, agent: &ureq::Agent, page_id: &str, children: &[Value]) -> Result<()> {
        let url = format!("https://api.notion.com/v1/blocks/{page_id}/children");
        agent
            .patch(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Notion-Version", NOTION_VERSION)
            .send_json(json!({ "children": children }))
            .map_err(|e| notion_err("notion 追記", e))?;
        Ok(())
    }
}

/// 連携トークンでアクセスできる書き出し先ページの候補（OAuth 同意時にユーザーが共有したページ）。
/// id は API が返すダッシュ付き UUID。フロントのドロップダウンに出し、選択を `notion_parent_id` に保存する。
#[derive(serde::Serialize)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
}

/// 検索（書き出し先候補の列挙）。
const API_SEARCH: &str = "https://api.notion.com/v1/search";

/// 連携トークンでアクセスできるページ一覧を返す（object=page のみ）。
/// OAuth 連携直後にユーザーが共有を許可したページがここに出る。DB（行）は page 親に使えない
/// （新 Notion-Version は data_source_id を要求）ため除外し、純粋なページのみを候補にする。
pub fn accessible_pages(token: &str) -> Result<Vec<NotionPage>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build();
    let resp = agent
        .post(API_SEARCH)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Notion-Version", NOTION_VERSION)
        .send_json(json!({
            "filter": { "value": "page", "property": "object" },
            "page_size": 100
        }))
        .map_err(|e| notion_err("notion ページ検索", e))?;
    let v: Value = resp
        .into_json()
        .map_err(|e| CoreError::Model(format!("notion json: {e}")))?;
    let mut pages = Vec::new();
    for r in v["results"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        if r["object"].as_str() != Some("page") {
            continue;
        }
        let Some(id) = r["id"].as_str() else { continue };
        let title = page_title(r).unwrap_or_else(|| "(無題)".to_string());
        pages.push(NotionPage {
            id: id.to_string(),
            title,
        });
    }
    Ok(pages)
}

/// page オブジェクトの properties から type=title のプロパティのテキストを取り出す。
/// （タイトルプロパティの名前はページにより異なるので type で探す。）
fn page_title(page: &Value) -> Option<String> {
    let props = page["properties"].as_object()?;
    for prop in props.values() {
        if prop["type"].as_str() == Some("title") {
            let text: String = prop["title"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t["plain_text"].as_str())
                .collect();
            let t = text.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Notion エラーを `error.export.*` の安定キーに整形する（表示文言はフロントの i18n 辞書が持ち、
/// 401=トークン無効 / 403・404=ページ未共有系 を原因別キーで区別する。これが無いと「トークン誤り」か
/// 「ページ未共有」かが分からない）。label・HTTP コード・応答ボディはキーの後ろに ": " で連結して
/// 詳細として原文のまま届ける（フロントは「文言 (詳細)」で表示する）。
fn notion_err(label: &str, e: ureq::Error) -> CoreError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: String = resp
                .into_string()
                .unwrap_or_default()
                .chars()
                .take(400)
                .collect();
            let key = match code {
                401 => "error.export.notion_unauthorized",
                403 | 404 => "error.export.notion_page_access",
                _ => "error.export.notion_api",
            };
            CoreError::Model(format!("{key}: {label} {code}: {body}"))
        }
        other => CoreError::Model(format!("error.export.notion_api: {label}: {other}")),
    }
}

/// ユーザーが貼る Notion ページ URL or 生 ID から 32 桁 ID を取り出しダッシュ整形する。
/// 例: `https://www.notion.so/My-Cafe-Notes-1f2e...32hex?v=...`（`?` 以降のビュー ID は除外）。
/// ID は URL 末尾に来るため、タイトル中の hex 文字（"Cafe" 等）は前方に落ち、末尾 32 桁が常に ID。
fn normalize_page_id(input: &str) -> Result<String> {
    let s = input.trim();
    if s.is_empty() {
        return Err(CoreError::Model(
            "error.export.notion_parent_missing".to_string(),
        ));
    }
    // `?`（クエリ＝ビュー ID も 32 hex）と `#`（フラグメント）より前だけを見る。
    let path = s.split(['?', '#']).next().unwrap_or(s);
    let hex: String = path.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() < 32 {
        return Err(CoreError::Model(format!(
            "Notion ページ ID を認識できません（32 桁 ID か共有ページの URL を貼ってください）: {input}"
        )));
    }
    let id = hex[hex.len() - 32..].to_ascii_lowercase();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &id[0..8],
        &id[8..12],
        &id[12..16],
        &id[16..20],
        &id[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_raw_dashed_and_url() {
        let id = "1f2e3d4c5b6a7980a1b2c3d4e5f6a7b8";
        let dashed = "1f2e3d4c-5b6a-7980-a1b2-c3d4e5f6a7b8";
        assert_eq!(normalize_page_id(id).unwrap(), dashed);
        assert_eq!(normalize_page_id(dashed).unwrap(), dashed);
        // タイトルに hex 語 "Cafe" を含み、末尾に `?v=<32hex ビュー ID>` が付く実 URL。
        let url = format!(
            "https://www.notion.so/My-Cafe-Notes-{id}?v=00000000000000000000000000000000&pvs=4"
        );
        assert_eq!(normalize_page_id(&url).unwrap(), dashed);
    }

    #[test]
    fn normalize_rejects_short_and_empty() {
        assert!(normalize_page_id("abc").is_err());
        assert!(normalize_page_id("   ").is_err());
    }
}
