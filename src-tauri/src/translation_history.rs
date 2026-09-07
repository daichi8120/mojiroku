//! Read the locally saved live-caption translations without aligning them to final STT.
use mojiroku_core::store::{SavedLiveTranslation, SqliteStore};
use tauri::State;

#[tauri::command]
pub(crate) fn list_live_translations(
    store: State<'_, SqliteStore>,
    id: String,
) -> Result<Vec<SavedLiveTranslation>, String> {
    store.list_live_translations(&id).map_err(|e| e.to_string())
}
