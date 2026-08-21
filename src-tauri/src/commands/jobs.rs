//! バックグラウンドジョブ投入コマンド（ADR-0024）。
//!
//! 既存録音を対象に「文字起こし」「後付け話者分離」を永続キューへ積み、即座に返す。
//! 実処理はワーカー（`crate::jobs`）が `HEAVY_ML_JOB` を 1 本ずつ取って回す。
//! **設定（言語・話者分離指定）は enqueue 時にスナップショット**して `JobParams` に固めるので、
//! キュー待機中に設定を変えても各ジョブは投入時点の指定で走る（ワーカーは live 設定を読まない）。

use super::*;
use crate::jobs::JobQueue;
use mojiroku_core::store::JobParams;
use tauri::State;

/// 現在のコンテンツ言語・STT 言語ヒントを `JobParams` へスナップショットする。
/// `super::insert_and_enqueue_transcribe`（録音停止・ファイル取り込みのフリップ）からも使う。
pub(crate) fn snapshot_params(app: &AppHandle, diarize: bool) -> Result<JobParams, String> {
    let cfg = load_settings(app)?;
    Ok(JobParams {
        diarize,
        stt_lang: cfg.effective_transcribe_language().map(str::to_string),
        lang: cfg.effective_language().to_string(),
    })
}

/// 既存録音を（再）文字起こしするジョブを投入する。`diarize=true` なら STT に話者分離も含める。
/// 録音のみ保存（音声だけ先に確定）した行を後から文字起こしする経路でもある（ADR-0024）。
#[tauri::command]
pub(crate) fn transcribe_recording(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    queue: State<'_, JobQueue>,
    recording_id: String,
    diarize: bool,
) -> Result<StartJobResult, String> {
    let params = snapshot_params(&app, diarize)?;
    let job_id = uuid::Uuid::new_v4().to_string();
    store
        .enqueue_job(&job_id, &recording_id, "transcribe", &params)
        .map_err(|e| e.to_string())?;
    queue.wake();
    Ok(StartJobResult {
        recording_id,
        job_id: Some(job_id),
    })
}

/// 既存録音に**後から話者分離**を掛けるジョブを投入する（ベスト努力で表示名を引き継ぐ・ADR-0024）。
/// 文字起こし済みが前提（本文が無ければワーカーがエラーにする）。File/Mic の単一トラック録音のみ対象。
///
/// **会議（Live）は拒否する**: 取得時に相手＝話者分離・自分＝ソース帰属で確定済みで、後から system 音声を
/// 再分離して全 transcript に merge すると自分セグメントが相手話者へ化けて壊れる（無意味かつ破壊的）。
#[tauri::command]
pub(crate) fn diarize_recording(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    queue: State<'_, JobQueue>,
    recording_id: String,
) -> Result<StartJobResult, String> {
    // 会議は enqueue 前に弾く（doomed なジョブ行を作らず即フィードバック）。
    let detail = resolve_recording_detail(&store, &recording_id)?;
    if matches!(detail.recording.source_type, mojiroku_core::SourceType::Live) {
        return Err("error.job.already_diarized".to_string());
    }
    // diarize ジョブに話者分離フラグは不要だが、params の形は共通なので false を入れる。
    let params = snapshot_params(&app, false)?;
    let job_id = uuid::Uuid::new_v4().to_string();
    store
        .enqueue_job(&job_id, &recording_id, "diarize", &params)
        .map_err(|e| e.to_string())?;
    queue.wake();
    Ok(StartJobResult {
        recording_id,
        job_id: Some(job_id),
    })
}

/// 進行中・要注意なジョブ一覧（pending/running/failed、更新の新しい順）。
/// UI のキュー表示・履歴行の処理中/失敗バッジ用。完了（done/canceled）は含めない。
#[tauri::command]
pub(crate) fn list_jobs(
    store: State<'_, SqliteStore>,
) -> Result<Vec<mojiroku_core::store::Job>, String> {
    store
        .list_jobs(&["pending", "running", "failed"])
        .map_err(|e| e.to_string())
}

/// pending ジョブをキャンセルする。running は中断不可（`spawn_blocking` 内）なので完走する
/// （キャンセルできたら true、できなかった=既に running/終端なら false）。
#[tauri::command]
pub(crate) fn cancel_job(store: State<'_, SqliteStore>, job_id: String) -> Result<bool, String> {
    store.cancel_job(&job_id).map_err(|e| e.to_string())
}
