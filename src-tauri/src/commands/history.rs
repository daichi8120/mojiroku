//! 履歴コマンド（一覧・全文検索・詳細・削除・改名）。

use super::*;
use tauri::State;

/// 履歴一覧（created_at 降順）。
#[tauri::command]
pub(crate) fn list_recordings(store: State<'_, SqliteStore>) -> Result<Vec<mojiroku_core::Recording>, String> {
    store.list_recordings().map_err(|e| e.to_string())
}

/// 履歴の全文検索（title + 本文）。空クエリは呼び出し側で list_recordings に切替える前提。
#[tauri::command]
pub(crate) fn search_recordings(
    store: State<'_, SqliteStore>,
    query: String,
) -> Result<Vec<mojiroku_core::store::SearchHit>, String> {
    store.search_recordings(&query).map_err(|e| e.to_string())
}

/// 履歴詳細（文字起こし＋全要約）。無ければ null。
#[tauri::command]
pub(crate) fn get_recording(
    store: State<'_, SqliteStore>,
    id: String,
) -> Result<Option<mojiroku_core::store::RecordingDetail>, String> {
    store.get_recording_detail(&id).map_err(|e| e.to_string())
}

/// 履歴 1 件削除（FK CASCADE で関連も削除）。録音原本があれば一緒に消す。
#[tauri::command]
pub(crate) fn delete_recording(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    id: String,
) -> Result<(), String> {
    store.delete_recording(&id).map_err(|e| e.to_string())?;
    // 録音原本（recordings/<id>.*）を best-effort で削除（孤立防止）。
    // file 取込=<id>.<元拡張子> / mic・結合ミックス=<id>.wav に加え、会議のデュアルトラック原本
    // <id>-mic.wav / <id>-system.wav（native rate・大容量）も消す（belongs_to_recording 参照）。
    if let Ok(rec_dir) = resolve_recordings_dir(&app) {
        if let Ok(entries) = std::fs::read_dir(&rec_dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| belongs_to_recording(n, &id))
                {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    Ok(())
}

/// recordings/ 内のファイル名が録音 `id` の原本かを判定する。
/// `<id>.<ext>`（file 取込・mic・会議の結合ミックス）と `<id>-mic.wav` / `<id>-system.wav`
/// （会議のトラック別原本）の両系統を対象にする。id は 36 文字 UUID 固定なので、別録音の
/// `<id2>.wav`（37 文字目は必ず `.`）が `<id>-` に一致することはなく、prefix 衝突は起きない。
fn belongs_to_recording(name: &str, id: &str) -> bool {
    name.starts_with(&format!("{id}.")) || name.starts_with(&format!("{id}-"))
}

/// 録音タイトルを変更する。`title` が null/空白なら既定の「無題」表示へ戻す（NULL）。
/// 全文検索（rec_fts）も同期されるので履歴検索と整合する。
#[tauri::command]
pub(crate) fn rename_recording(
    store: State<'_, SqliteStore>,
    id: String,
    title: Option<String>,
) -> Result<(), String> {
    store
        .rename_recording(&id, title.as_deref())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::belongs_to_recording;

    #[test]
    fn belongs_to_recording_matches_all_track_wavs_without_collision() {
        let id = "11111111-1111-4111-8111-111111111111";
        let other = "22222222-2222-4222-8222-222222222222";
        // 会議の 3 本（結合ミックス＋トラック別）＋ file 取込がすべて一致する。
        assert!(belongs_to_recording(&format!("{id}.wav"), id));
        assert!(belongs_to_recording(&format!("{id}-mic.wav"), id));
        assert!(belongs_to_recording(&format!("{id}-system.wav"), id));
        assert!(belongs_to_recording(&format!("{id}.m4a"), id));
        // 別録音のファイルは巻き込まない（UUID なので prefix 衝突しない）。
        assert!(!belongs_to_recording(&format!("{other}.wav"), id));
        assert!(!belongs_to_recording(&format!("{other}-mic.wav"), id));
    }
}
