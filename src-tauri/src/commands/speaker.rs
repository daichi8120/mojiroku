//! 話者コマンド（録音内の話者改名 ＋ 話者ライブラリのクロス会議照合・ADR-0018）。

use super::*;
use tauri::State;

/// 話者の表示名（改名）を更新する。`display_name` が null/空なら既定ラベルへ戻す。
#[tauri::command]
pub(crate) fn rename_speaker(
    store: State<'_, SqliteStore>,
    recording_id: String,
    speaker_id: String,
    display_name: Option<String>,
) -> Result<(), String> {
    // 空白のみは「改名なし」とみなして NULL に正規化する。
    let name = display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    store
        .rename_speaker(&recording_id, &speaker_id, name)
        .map_err(|e| e.to_string())
}

// ── 話者ライブラリ（クロス会議の声紋照合・ADR-0018） ──────────────────────────

/// 端末内の登録話者一覧（名前昇順・対応づけ数つき）。
#[tauri::command]
pub(crate) fn list_speaker_library(
    store: State<'_, SqliteStore>,
) -> Result<Vec<mojiroku_core::store::LibrarySpeaker>, String> {
    store.list_library_speakers().map_err(|e| e.to_string())
}

/// 話者ライブラリに人物を新規登録し、採番した id を返す。
#[tauri::command]
pub(crate) fn add_speaker_to_library(
    store: State<'_, SqliteStore>,
    name: String,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("error.speaker.name_empty".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    store
        .add_library_speaker(&id, name)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// 登録話者の改名。
#[tauri::command]
pub(crate) fn rename_speaker_library(
    store: State<'_, SqliteStore>,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("error.speaker.name_empty".into());
    }
    store
        .rename_library_speaker(&id, name)
        .map_err(|e| e.to_string())
}

/// 登録話者の削除（対応づけも CASCADE で消える）。
#[tauri::command]
pub(crate) fn delete_speaker_library(store: State<'_, SqliteStore>, id: String) -> Result<(), String> {
    store.delete_library_speaker(&id).map_err(|e| e.to_string())
}

/// 録音の各話者を話者ライブラリへ 1:N 照合（サジェスト先行）。confidence/margin を返す。
#[tauri::command]
pub(crate) fn identify_speakers(
    store: State<'_, SqliteStore>,
    recording_id: String,
) -> Result<Vec<mojiroku_core::store::SpeakerMatchSuggestion>, String> {
    store
        .identify_speakers(&recording_id)
        .map_err(|e| e.to_string())
}

/// 録音話者をライブラリ人物へ対応づけ（サジェスト採用・確定）。
#[tauri::command]
pub(crate) fn link_speaker(
    store: State<'_, SqliteStore>,
    recording_id: String,
    speaker_id: String,
    library_id: String,
    confidence: f64,
) -> Result<(), String> {
    store
        .link_speaker(&recording_id, &speaker_id, &library_id, confidence)
        .map_err(|e| e.to_string())
}

/// 録音話者の対応づけを解除。
#[tauri::command]
pub(crate) fn unlink_speaker(
    store: State<'_, SqliteStore>,
    recording_id: String,
    speaker_id: String,
) -> Result<(), String> {
    store
        .unlink_speaker(&recording_id, &speaker_id)
        .map_err(|e| e.to_string())
}
