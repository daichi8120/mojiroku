//! 議事録（`RecordingDetail`）を Notion のブロック JSON へ変換する純粋層。
//!
//! `mod.rs`（Notion HTTP/API クライアント）から分離。ネットワーク I/O を持たず、
//! `RecordingDetail` → `Vec<Value>`（Notion ブロック配列）の変換だけを担う（単体テスト容易）。
//! ⚠️ インライン装飾の除去・見出し内 `#` 保持などのプラットフォーム差は
//! `crate::export::common` の doc（parse_heading を共有しない理由）も参照。

use super::RICH_TEXT_LIMIT;
use crate::export::common::{parse_bullet, template_label};
use crate::lang::Lang;
use crate::store::RecordingDetail;
use serde_json::{json, Value};

/// 段落 1 ブロックあたりの文字数上限（< 2000 にして rich_text を 1 要素に収める）。
/// 話者分離なしの長尺文字起こしが 1 段落へ collapse して壁テキスト/巨大 rich_text 配列に
/// なるのを防ぐ（Notion の rich_text 配列 100 要素・リクエストサイズ上限の保険も兼ねる）。
const PARAGRAPH_CHAR_LIMIT: usize = 1800;

/// 議事録ページの本文ブロックを組む（要約セクション → 区切り → 文字起こし）。
/// 見出し・既定文言は `lang` に追従する（内容自体は保存時のまま）。
pub(super) fn build_blocks(detail: &RecordingDetail, lang: Lang) -> Vec<Value> {
    let mut blocks: Vec<Value> = Vec::new();

    // 要約セクション（議事録 / 要約 / アクションアイテム）。各 content は LLM の Markdown。
    for s in &detail.summaries {
        blocks.push(heading_block(2, template_label(&s.template_id, lang)));
        blocks.extend(md_to_blocks(&s.content));
    }

    // 文字起こし（同一話者の連続セグメントをマージしてブロック数を抑える）。
    if !detail.transcript.segments.is_empty() {
        let turns = merged_turns(detail);
        // 中身のあるターンが 1 つでもあるときだけ見出し/区切りを出す（空ターンしか無ければ省く）。
        if turns.iter().any(|(_, text)| !text.is_empty()) {
            blocks.push(divider_block());
            blocks.push(heading_block(
                2,
                match lang {
                    Lang::Ja => "文字起こし",
                    Lang::En => "Transcript",
                },
            ));
            for (speaker_id, text) in turns {
                if text.is_empty() {
                    continue; // 空テキストのターンは段落を作らない
                }
                // 表示名はここ（描画時）で解決する。マージ判定は speaker_id で行うため、
                // 別話者が同一表示名でも別ターンに保たれる。
                let line = match speaker_id {
                    Some(id) => format!("{}: {}", speaker_display(id, &detail.speakers), text),
                    None => text,
                };
                push_paragraphs(&mut blocks, &line);
            }
        }
    }

    if blocks.is_empty() {
        blocks.push(paragraph_block(match lang {
            Lang::Ja => "（内容なし）",
            Lang::En => "(empty)",
        }));
    }
    blocks
}

/// 長い行を PARAGRAPH_CHAR_LIMIT ごとに分割して複数の段落ブロックへ。
/// 話者分離なしで全文が 1 ターンへ collapse しても壁テキスト/巨大 rich_text にならないようにする。
fn push_paragraphs(blocks: &mut Vec<Value>, line: &str) {
    let chars: Vec<char> = line.chars().collect();
    for chunk in chars.chunks(PARAGRAPH_CHAR_LIMIT) {
        let s: String = chunk.iter().collect();
        blocks.push(paragraph_block(&s));
    }
}

/// 連続する同一話者セグメントを 1 ターンへ結合（600 セグメント → ~数十ブロック）。
/// 結合判定は **speaker_id** で行う（表示名ではない）。別話者を同じ表示名へ改名しても
/// 別ターンに保つため。表示名の解決は呼び出し側（build_blocks）の描画時に行う。
fn merged_turns(detail: &RecordingDetail) -> Vec<(Option<&str>, String)> {
    let mut turns: Vec<(Option<&str>, String)> = Vec::new();
    for seg in &detail.transcript.segments {
        let who = seg.speaker_id.as_deref();
        let text = seg.text.trim();
        match turns.last_mut() {
            Some((prev, buf)) if *prev == who => {
                if !text.is_empty() {
                    if !buf.is_empty() {
                        buf.push(' ');
                    }
                    buf.push_str(text);
                }
            }
            _ => turns.push((who, text.to_string())),
        }
    }
    turns
}

/// speaker_id → 表示名（display_name ?? label ?? id）。frontend speakerName と同等。
fn speaker_display(id: &str, speakers: &[crate::schemas::Speaker]) -> String {
    speakers
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.display_name.clone().unwrap_or_else(|| s.label.clone()))
        .unwrap_or_else(|| id.to_string())
}

/// 要約 Markdown を Notion ブロックへ（ブロックレベルのみ変換）。
/// 見出し `#`/`##`/`###`（`####`+ は heading_3 に丸め）、箇条書き `-`/`*`/`+`、他は段落。
/// ⚠️ インライン装飾（`**太字**` 等）は除去してプレーン化する（MVP の制限。UI で開示）。
fn md_to_blocks(md: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    for raw in md.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((level, rest)) = parse_heading(line) {
            blocks.push(heading_block(level, &strip_inline(rest)));
        } else if let Some(rest) = parse_bullet(line) {
            blocks.push(bullet_block(&strip_inline(rest)));
        } else {
            blocks.push(paragraph_block(&strip_inline(line)));
        }
    }
    blocks
}

/// `# 見出し` を (level, text) に。`#tag`（後ろにスペースなし）は見出し扱いしない。`####`+ は 3 に丸め。
fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 {
        return None;
    }
    let rest = line[hashes..].strip_prefix(' ')?;
    Some((hashes.min(3) as u8, rest.trim()))
}

/// インライン Markdown 装飾の除去（太字 `**`/`__`、コード `` ` `` の記号を取り除きプレーン化）。
fn strip_inline(s: &str) -> String {
    s.replace("**", "").replace("__", "").replace('`', "")
}

fn heading_block(level: u8, text: &str) -> Value {
    let key = match level {
        1 => "heading_1",
        2 => "heading_2",
        _ => "heading_3",
    };
    json!({ "object": "block", "type": key, key: { "rich_text": rich_text(text) } })
}

fn paragraph_block(text: &str) -> Value {
    json!({ "object": "block", "type": "paragraph", "paragraph": { "rich_text": rich_text(text) } })
}

fn bullet_block(text: &str) -> Value {
    json!({ "object": "block", "type": "bulleted_list_item", "bulleted_list_item": { "rich_text": rich_text(text) } })
}

fn divider_block() -> Value {
    json!({ "object": "block", "type": "divider", "divider": {} })
}

/// テキストを Notion rich_text 配列へ。1 要素 2000 文字（char 単位、マルチバイト安全）でチャンクし、
/// 長い段落でも 400（rich_text too long）を踏まないようにする。
fn rich_text(text: &str) -> Vec<Value> {
    if text.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(RICH_TEXT_LIMIT)
        .map(|chunk| {
            let content: String = chunk.iter().collect();
            json!({ "type": "text", "text": { "content": content } })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::{Recording, Segment, SourceType, Speaker, Summary, Transcript};

    #[test]
    fn md_maps_headings_bullets_paragraph() {
        let blocks = md_to_blocks("# 見出し\n\n- 箇条\n本文\n#### 深い見出し");
        assert_eq!(blocks.len(), 4); // 空行はスキップ
        assert_eq!(blocks[0]["type"], "heading_1");
        assert_eq!(blocks[1]["type"], "bulleted_list_item");
        assert_eq!(blocks[2]["type"], "paragraph");
        assert_eq!(blocks[3]["type"], "heading_3"); // ####+ は heading_3 に丸め
    }

    #[test]
    fn strip_inline_removes_bold_markers() {
        assert_eq!(strip_inline("**重要**な`点`"), "重要な点");
    }

    /// characterization: notion の `parse_heading` は見出し内に残った `#` を**保持**する
    /// （`## # 議題` → `(2, "# 議題")`。`strip_inline` は `#` を消さない）。slack は全 `#` を
    /// 剥がす（→ `"議題"`）意図的なプラットフォーム差で、common 化で潰さないことを固定する。
    #[test]
    fn heading_keeps_inner_hash_unlike_slack() {
        assert_eq!(parse_heading("## # 議題"), Some((2, "# 議題")));
        let blocks = md_to_blocks("## # 議題");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "heading_2");
        assert_eq!(
            blocks[0]["heading_2"]["rich_text"][0]["text"]["content"],
            "# 議題"
        );
        // `#tag`（直後に空白なし）は見出し扱いしない。
        assert_eq!(parse_heading("#tag です"), None);
    }

    #[test]
    fn rich_text_chunks_over_limit() {
        let long = "あ".repeat(2500);
        assert_eq!(rich_text(&long).len(), 2); // 2000 + 500
        assert!(rich_text("").is_empty());
    }

    #[test]
    fn merged_turns_collapses_consecutive_same_speaker() {
        let detail = RecordingDetail {
            recording: Recording {
                id: "r1".into(),
                source_type: SourceType::File,
                title: Some("t".into()),
                duration_ms: 1000,
                sample_rate: 16000,
                created_at: "2026-06-27T00:00:00Z".into(),
            },
            transcript: Transcript {
                language: None,
                segments: vec![
                    seg("あ", Some("s1")),
                    seg("い", Some("s1")),
                    seg("う", Some("s2")),
                ],
            },
            summaries: vec![Summary {
                template_id: "minutes".into(),
                content: "# 見出し\n本文".into(),
                action_items: vec![],
                stale: false,
            }],
            speakers: vec![
                Speaker {
                    id: "s1".into(),
                    label: "話者1".into(),
                    display_name: Some("田中".into()),
                },
                Speaker {
                    id: "s2".into(),
                    label: "話者2".into(),
                    display_name: None,
                },
            ],
            active_job: None,
        };
        // 結合キーは speaker_id（表示名ではない）。
        let turns = merged_turns(&detail);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0], (Some("s1"), "あ い".to_string()));
        assert_eq!(turns[1], (Some("s2"), "う".to_string()));

        // build_blocks: 要約セクション(見出し2 + 本文 md の 見出し1 + 段落 = 3)
        //              + divider + 文字起こし見出し + 2 ターン = 7 ブロック
        let blocks = build_blocks(&detail, Lang::Ja);
        assert_eq!(blocks.len(), 7);
        assert_eq!(blocks[0]["type"], "heading_2"); // "議事録"
        assert_eq!(blocks[0]["heading_2"]["rich_text"][0]["text"]["content"], "議事録");
        assert_eq!(blocks[4]["heading_2"]["rich_text"][0]["text"]["content"], "文字起こし");
        assert_eq!(blocks[1]["type"], "heading_1"); // 本文 md の "# 見出し"
        // 表示名は描画時に解決される（s1 → "田中"）。
        assert_eq!(blocks[6]["paragraph"]["rich_text"][0]["text"]["content"], "話者2: う");
        assert_eq!(blocks[5]["paragraph"]["rich_text"][0]["text"]["content"], "田中: あ い");
    }

    /// #5: 別 speaker_id が同じ表示名（両方「田中」）でも、連続マージで 1 ターンに潰れない。
    #[test]
    fn different_speakers_same_display_name_stay_separate() {
        let speakers = vec![
            Speaker { id: "s1".into(), label: "話者1".into(), display_name: Some("田中".into()) },
            Speaker { id: "s2".into(), label: "話者2".into(), display_name: Some("田中".into()) },
        ];
        let detail = detail_with(
            vec![seg("あ", Some("s1")), seg("い", Some("s2"))],
            speakers,
            vec![],
        );
        let turns = merged_turns(&detail);
        assert_eq!(turns.len(), 2); // speaker_id が違うので別ターン
        // 文字起こしブロックは 2 段落（同名でも分かれる）。
        let blocks = build_blocks(&detail, Lang::Ja);
        let paras: Vec<_> = blocks.iter().filter(|b| b["type"] == "paragraph").collect();
        assert_eq!(paras.len(), 2);
    }

    /// en: 見出し・既定文言が英語になる（見出しは frontend templates.ts と一致）。
    #[test]
    fn build_blocks_english_headings_and_fallbacks() {
        let detail = detail_with(
            vec![seg("hello", None)],
            vec![],
            vec![Summary {
                template_id: "minutes".into(),
                content: "body".into(),
                action_items: vec![],
                stale: false,
            }],
        );
        let blocks = build_blocks(&detail, Lang::En);
        assert_eq!(blocks[0]["heading_2"]["rich_text"][0]["text"]["content"], "Minutes");
        assert_eq!(blocks[3]["heading_2"]["rich_text"][0]["text"]["content"], "Transcript");

        // 中身が何も無ければ "(empty)" 段落 1 つだけ。
        let empty = build_blocks(&detail_with(vec![], vec![], vec![]), Lang::En);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0]["paragraph"]["rich_text"][0]["text"]["content"], "(empty)");
    }

    /// #6: 空テキストのセグメントは段落を生まない。文字起こしが空ターンのみなら見出しも出さない。
    #[test]
    fn empty_text_segments_produce_no_blocks() {
        let detail = detail_with(
            vec![seg("   ", Some("s1")), seg("", None)],
            vec![Speaker { id: "s1".into(), label: "話者1".into(), display_name: None }],
            vec![],
        );
        let blocks = build_blocks(&detail, Lang::Ja);
        // 要約も無いので「（内容なし）」段落 1 つだけ（divider/文字起こし見出しは出ない）。
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["paragraph"]["rich_text"][0]["text"]["content"], "（内容なし）");
    }

    /// #4: 1 ターンが長大でも PARAGRAPH_CHAR_LIMIT ごとに複数段落へ分割される（壁テキスト回避）。
    #[test]
    fn long_turn_splits_into_multiple_paragraphs() {
        let long = "あ".repeat(PARAGRAPH_CHAR_LIMIT * 2 + 10); // 話者なし → 全文 1 ターン
        let detail = detail_with(vec![seg(&long, None)], vec![], vec![]);
        let blocks = build_blocks(&detail, Lang::Ja);
        let paras: Vec<_> = blocks.iter().filter(|b| b["type"] == "paragraph").collect();
        assert_eq!(paras.len(), 3); // 1800 + 1800 + 10
        // 各段落の rich_text は 1 要素（PARAGRAPH_CHAR_LIMIT < 2000 のため）。
        for p in paras {
            assert_eq!(p["paragraph"]["rich_text"].as_array().unwrap().len(), 1);
        }
    }

    fn detail_with(
        segments: Vec<Segment>,
        speakers: Vec<Speaker>,
        summaries: Vec<Summary>,
    ) -> RecordingDetail {
        RecordingDetail {
            recording: Recording {
                id: "r1".into(),
                source_type: SourceType::File,
                title: Some("t".into()),
                duration_ms: 1000,
                sample_rate: 16000,
                created_at: "2026-06-27T00:00:00Z".into(),
            },
            transcript: Transcript { language: None, segments },
            summaries,
            speakers,
            active_job: None,
        }
    }

    fn seg(text: &str, speaker: Option<&str>) -> Segment {
        Segment {
            idx: 0,
            start_ms: 0,
            end_ms: 0,
            text: text.into(),
            speaker_id: speaker.map(|s| s.into()),
        }
    }
}
