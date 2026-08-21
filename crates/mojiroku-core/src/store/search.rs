//! 履歴の全文検索（FTS5 trigram / LIKE フォールバック）。mod.rs から分割。
use super::*;

impl SqliteStore {
    /// 履歴の全文検索（title + 本文）。録音単位・重複なしで返す。
    /// - 空/空白クエリ → 空 Vec
    /// - 3 文字以上 → FTS5 trigram MATCH（snippet 付き）
    /// - 1-2 文字 → LIKE フォールバック（trigram の最小トークン長は 3 のため）
    pub fn search_recordings(&self, query: &str) -> Result<Vec<SearchHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        // trigram は「文字」単位。chars().count()（コードポイント数）で判定（バイト長は不可）。
        if q.chars().count() >= 3 {
            search_fts(&conn, q)
        } else {
            search_like(&conn, q)
        }
    }
}

/// FTS5 trigram 経路（3 文字以上）。MATCH をフレーズ化して演算子を無効化し、部分一致にする。
/// 列 0-5 は recordings（row_to_recording と一致）、列 6 は body の snippet。
fn search_fts(conn: &Connection, q: &str) -> Result<Vec<SearchHit>> {
    // " で包み内部の " を "" にエスケープ → 全体を 1 フレーズ扱い（AND/OR/NEAR/*/: を無効化）。
    let match_expr = format!("\"{}\"", q.replace('"', "\"\""));
    let mut stmt = conn.prepare(
        "SELECT r.id, r.source_type, r.title, r.duration_ms, r.sample_rate, r.created_at,
                snippet(rec_fts, 1, '[', ']', '…', 12)
         FROM rec_fts f
         JOIN recordings r ON r.id = f.recording_id
         WHERE rec_fts MATCH ?1
         ORDER BY rank",
    )?;
    let rows = stmt.query_map(params![match_expr], |r| {
        let mut snippet: String = r.get(6)?;
        // タイトルのみマッチで body snippet が空のときは title を見せる。
        if snippet.is_empty() {
            snippet = r.get::<_, Option<String>>(2)?.unwrap_or_default();
        }
        Ok(SearchHit {
            recording: row_to_recording(r)?,
            snippet,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// LIKE フォールバック（1-2 文字、trigram の最小長 3 未満）。% _ \ をエスケープ。
fn search_like(conn: &Connection, q: &str) -> Result<Vec<SearchHit>> {
    let pat = format!(
        "%{}%",
        q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );
    let mut stmt = conn.prepare(
        "SELECT r.id, r.source_type, r.title, r.duration_ms, r.sample_rate, r.created_at,
                (SELECT s.text FROM segments s
                 WHERE s.recording_id = r.id AND s.text LIKE ?1 ESCAPE '\\'
                 ORDER BY s.idx ASC LIMIT 1)
         FROM recordings r
         WHERE EXISTS (SELECT 1 FROM segments s
                       WHERE s.recording_id = r.id AND s.text LIKE ?1 ESCAPE '\\')
            OR COALESCE(r.title, '') LIKE ?1 ESCAPE '\\'
         ORDER BY r.created_at DESC",
    )?;
    let rows = stmt.query_map(params![pat], |r| {
        let snippet: Option<String> = r.get(6)?;
        let snippet = match snippet {
            Some(s) => s,
            None => r.get::<_, Option<String>>(2)?.unwrap_or_default(), // title のみマッチ
        };
        Ok(SearchHit {
            recording: row_to_recording(r)?,
            snippet,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
