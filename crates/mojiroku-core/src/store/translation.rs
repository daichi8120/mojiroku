use super::*;
use std::collections::HashSet;

pub(super) const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS live_translations (
  recording_id TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
  idx INTEGER NOT NULL,
  source_id INTEGER NOT NULL CHECK (source_id >= 0),
  source_text TEXT NOT NULL,
  target TEXT NOT NULL CHECK (target IN ('ja', 'en')),
  translation TEXT NOT NULL,
  PRIMARY KEY (recording_id, source_id, target),
  UNIQUE (recording_id, idx)
);
"#;

/// Exact live caption and completed translation; not aligned to the later offline transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLiveTranslation {
    pub source_id: i64,
    pub source_text: String,
    pub target: String,
    pub translation: String,
}

pub fn validate_live_translations(rows: &[SavedLiveTranslation]) -> Result<()> {
    let mut keys = HashSet::new();
    let mut bytes = 0;
    for row in rows {
        bytes += row.source_text.len() + row.translation.len();
        if row.source_id < 0
            || !matches!(row.target.as_str(), "ja" | "en")
            || row.source_text.trim().is_empty()
            || row.source_text.len() > 4096
            || row.translation.trim().is_empty()
            || row.translation.len() > 16_384
            || !keys.insert((row.source_id, row.target.as_str()))
        {
            return Err(CoreError::Db("Invalid live translation snapshot".into()));
        }
    }
    if rows.len() > 20_000 || bytes > 32 * 1024 * 1024 {
        return Err(CoreError::Db(
            "Live translation history is too large".into(),
        ));
    }
    Ok(())
}

pub(super) fn insert_rows(
    tx: &rusqlite::Transaction<'_>,
    recording_id: &str,
    rows: &[SavedLiveTranslation],
) -> Result<()> {
    let mut statement = tx.prepare(
        "INSERT INTO live_translations
         (recording_id, idx, source_id, source_text, target, translation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (idx, row) in rows.iter().enumerate() {
        statement.execute(params![
            recording_id,
            idx as i64,
            row.source_id,
            row.source_text,
            row.target,
            row.translation
        ])?;
    }
    Ok(())
}

impl SqliteStore {
    pub fn list_live_translations(&self, recording_id: &str) -> Result<Vec<SavedLiveTranslation>> {
        let conn = self.conn();
        let mut statement = conn.prepare(
            "SELECT source_id, source_text, target, translation FROM live_translations
             WHERE recording_id = ?1 ORDER BY idx",
        )?;
        let rows = statement.query_map([recording_id], |r| {
            Ok(SavedLiveTranslation {
                source_id: r.get(0)?,
                source_text: r.get(1)?,
                target: r.get(2)?,
                translation: r.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Recording {
        Recording {
            id: "meeting".into(),
            source_type: SourceType::Live,
            title: Some("Meeting".into()),
            duration_ms: 1000,
            sample_rate: 16000,
            created_at: "2026-09-07T00:00:00Z".into(),
        }
    }

    fn snapshot() -> SavedLiveTranslation {
        SavedLiveTranslation {
            source_id: 1,
            source_text: "Thank you.".into(),
            target: "en".into(),
            translation: "Thank you.".into(),
        }
    }

    #[test]
    fn snapshots_survive_transcript_replacement_and_recording_deletion_cascades() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .insert_recording_with_translations(&record(), &[snapshot()])
            .unwrap();
        store
            .replace_transcript(
                "meeting",
                &Transcript {
                    language: Some("en".into()),
                    segments: vec![],
                },
                &[],
            )
            .unwrap();
        assert_eq!(
            store.list_live_translations("meeting").unwrap(),
            vec![snapshot()]
        );
        store.delete_recording("meeting").unwrap();
        assert!(store.list_live_translations("meeting").unwrap().is_empty());
    }

    #[test]
    fn invalid_snapshot_does_not_insert_the_recording() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store
            .insert_recording_with_translations(&record(), &[snapshot(), snapshot()])
            .is_err());
        assert_eq!(
            store
                .conn()
                .query_row("SELECT count(*) FROM recordings", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn sql_failure_rolls_back_recording_and_search_row() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.conn().execute_batch("CREATE TRIGGER fail_translation BEFORE INSERT ON live_translations BEGIN SELECT RAISE(FAIL, 'simulated write failure'); END;").unwrap();
        assert!(store
            .insert_recording_with_translations(&record(), &[snapshot()])
            .is_err());
        for table in ["recordings", "rec_fts", "live_translations"] {
            assert_eq!(
                store
                    .conn()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn migration_preserves_existing_recordings_and_history_survives_reopen() {
        let path = std::env::temp_dir().join(format!(
            "mojiroku-translation-history-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let store = SqliteStore::open(&path).unwrap();
            store.insert_recording_only(&record()).unwrap();
            store
                .conn()
                .execute_batch("DROP TABLE live_translations; PRAGMA user_version=6;")
                .unwrap();
        }
        {
            let store = SqliteStore::open(&path).unwrap();
            assert!(store.list_live_translations("meeting").unwrap().is_empty());
            let mut next = record();
            next.id = "next".into();
            store
                .insert_recording_with_translations(&next, &[snapshot()])
                .unwrap();
        }
        {
            let store = SqliteStore::open(&path).unwrap();
            assert_eq!(
                store.list_live_translations("next").unwrap(),
                vec![snapshot()]
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
