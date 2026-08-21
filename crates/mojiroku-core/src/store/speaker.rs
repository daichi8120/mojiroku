//! 話者の改名 + 話者ライブラリ（クロス会議の声紋照合・ADR-0018）。mod.rs から分割。
use super::*;

impl SqliteStore {
    /// 話者の表示名（改名）を更新する。`display_name` が None なら既定ラベルへ戻す（NULL）。
    /// 該当録音・話者が無ければ 0 行更新で no-op（エラーにしない）。
    pub fn rename_speaker(
        &self,
        recording_id: &str,
        speaker_id: &str,
        display_name: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE speakers SET display_name = ?3
             WHERE recording_id = ?1 AND speaker_id = ?2",
            params![recording_id, speaker_id, display_name],
        )?;
        Ok(())
    }

    /// 話者ごとの声紋（重心, L2 正規化済み）を保存する。`save_recording` 後に呼ぶ派生データ。
    /// 同一 (recording, speaker) は置換。空なら no-op。`model` は埋め込みモデル名（差し替え検知用）。
    pub fn save_speaker_embeddings(
        &self,
        recording_id: &str,
        embeddings: &[crate::diarization::SpeakerEmbedding],
        model: &str,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO speaker_embeddings
                   (recording_id, speaker_id, vector, model, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in embeddings {
                stmt.execute(params![
                    recording_id,
                    e.speaker_id,
                    f32_to_blob(&e.vector),
                    model,
                    e.duration_ms as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// ライブラリ人物を 1 件登録。`id` はアプリ層が採番（UUID）。
    pub fn add_library_speaker(&self, id: &str, name: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO speaker_library (id, name) VALUES (?1, ?2)",
            params![id, name],
        )?;
        Ok(())
    }

    /// 登録話者の一覧（名前昇順）。`identified_count` は対応づけ済み録音話者数。
    pub fn list_library_speakers(&self) -> Result<Vec<LibrarySpeaker>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT l.id, l.name, COUNT(m.library_id)
               FROM speaker_library l
               LEFT JOIN speaker_matches m ON m.library_id = l.id
              GROUP BY l.id, l.name
              ORDER BY l.name ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LibrarySpeaker {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    identified_count: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 登録話者の改名。
    pub fn rename_library_speaker(&self, id: &str, name: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE speaker_library SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// 登録話者の削除（FK CASCADE で speaker_matches も消える）。
    pub fn delete_library_speaker(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM speaker_library WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 録音話者をライブラリ人物へ対応づけ（確定 or サジェスト採用）。同一話者は置換。
    pub fn link_speaker(
        &self,
        recording_id: &str,
        speaker_id: &str,
        library_id: &str,
        confidence: f64,
    ) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO speaker_matches
               (recording_id, speaker_id, library_id, confidence)
             VALUES (?1, ?2, ?3, ?4)",
            params![recording_id, speaker_id, library_id, confidence],
        )?;
        Ok(())
    }

    /// 録音話者の対応づけを解除。
    pub fn unlink_speaker(&self, recording_id: &str, speaker_id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM speaker_matches WHERE recording_id = ?1 AND speaker_id = ?2",
            params![recording_id, speaker_id],
        )?;
        Ok(())
    }

    /// 録音の各話者を話者ライブラリへ 1:N 照合（サジェスト先行・ADR-0018）。
    /// - ライブラリ人物の声紋 = その人物へ対応づけ済み話者声紋の平均（L2）。**現録音は除外**
    ///   （leave-one-recording-out。同一録音での自明な一致を避け、スパイクの評価系と一致させる）。
    /// - 最小エンロール尺未満の話者は `below_enroll_gate=true` で候補を返さない。
    /// - τ で機械確定はしない（confidence/margin を返し UI/ユーザーが判断）。
    pub fn identify_speakers(&self, recording_id: &str) -> Result<Vec<SpeakerMatchSuggestion>> {
        let conn = self.conn();

        // 現録音の話者声紋（speaker_id, vector, duration_ms）。
        let rec_speakers: Vec<(String, Vec<f32>, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT speaker_id, vector, duration_ms FROM speaker_embeddings
                 WHERE recording_id = ?1 ORDER BY speaker_id ASC",
            )?;
            let rows = stmt
                .query_map(params![recording_id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        blob_to_f32(&r.get::<_, Vec<u8>>(1)?),
                        r.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };

        // 既存の確定リンク（speaker_id -> library_id）。
        let linked: HashMap<String, String> = {
            let mut stmt = conn
                .prepare("SELECT speaker_id, library_id FROM speaker_matches WHERE recording_id = ?1")?;
            let rows = stmt
                .query_map(params![recording_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().collect()
        };

        // ライブラリ人物 → 声紋集合（現録音は除外）。matches を embeddings に join。
        let mut lib_vecs: HashMap<String, (String, Vec<Vec<f32>>)> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT m.library_id, l.name, e.vector
                   FROM speaker_matches m
                   JOIN speaker_library l ON l.id = m.library_id
                   JOIN speaker_embeddings e
                     ON e.recording_id = m.recording_id AND e.speaker_id = m.speaker_id
                  WHERE m.recording_id <> ?1",
            )?;
            let rows = stmt.query_map(params![recording_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    blob_to_f32(&r.get::<_, Vec<u8>>(2)?),
                ))
            })?;
            for row in rows {
                let (lib_id, name, vec) = row?;
                lib_vecs.entry(lib_id).or_insert_with(|| (name, Vec::new())).1.push(vec);
            }
        }
        // 人物ごとに重心（平均 → L2）。
        let library: Vec<(String, String, Vec<f32>)> = lib_vecs
            .into_iter()
            .map(|(id, (name, vecs))| (id, name, l2_mean(&vecs)))
            .collect();

        let mut out = Vec::with_capacity(rec_speakers.len());
        for (sid, vec, dur) in rec_speakers {
            let linked_library_id = linked.get(&sid).cloned();
            if (dur as u64) < MIN_ENROLL_MS {
                out.push(SpeakerMatchSuggestion {
                    speaker_id: sid,
                    linked_library_id,
                    top_library_id: None,
                    top_name: None,
                    confidence: None,
                    margin: None,
                    below_enroll_gate: true,
                });
                continue;
            }
            // cosine ランキング（vec も centroid も L2 正規化済み → dot = cosine）。
            let mut scored: Vec<(f64, &str, &str)> = library
                .iter()
                .filter(|(_, _, c)| c.len() == vec.len()) // 次元不一致（別モデル）は除外
                .map(|(id, name, c)| (dot(&vec, c) as f64, id.as_str(), name.as_str()))
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            let (top_library_id, top_name, confidence, margin) = match scored.first() {
                Some(&(s1, id, name)) => (
                    Some(id.to_string()),
                    Some(name.to_string()),
                    Some(s1),
                    scored.get(1).map(|&(s2, ..)| s1 - s2),
                ),
                None => (None, None, None, None),
            };
            out.push(SpeakerMatchSuggestion {
                speaker_id: sid,
                linked_library_id,
                top_library_id,
                top_name,
                confidence,
                margin,
                below_enroll_gate: false,
            });
        }
        Ok(out)
    }
}
