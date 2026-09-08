//! Application-owned model preparation. Views subscribe; they never own the transfer.
use mojiroku_core::models;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
pub(crate) struct DownloadSnapshot {
    pub file: String,
    pub size_bytes: u64,
    pub downloaded_bytes: u64,
    pub status: String,
    pub error: Option<String>,
}

type DownloadResult = Option<Result<PathBuf, String>>;
struct Transfer {
    snapshot: DownloadSnapshot,
    result: watch::Sender<DownloadResult>,
}
#[derive(Default)]
pub(crate) struct ModelDownloadsState {
    transfers: Arc<Mutex<HashMap<String, Transfer>>>,
}

impl ModelDownloadsState {
    fn reserve(
        &self,
        snapshot: DownloadSnapshot,
    ) -> (
        watch::Receiver<DownloadResult>,
        Option<watch::Sender<DownloadResult>>,
    ) {
        let mut transfers = self.transfers.lock().unwrap();
        if let Some(transfer) = transfers.get(&snapshot.file) {
            if transfer.result.borrow().is_none() {
                return (transfer.result.subscribe(), None);
            }
        }
        let (sender, receiver) = watch::channel(None);
        transfers.insert(
            snapshot.file.clone(),
            Transfer {
                snapshot,
                result: sender.clone(),
            },
        );
        (receiver, Some(sender))
    }
}

fn catalog_size(file: &str) -> Option<u64> {
    if file == models::TRANSLATION_MODEL_FILE {
        return Some(models::TRANSLATION_MODEL_BYTES);
    }
    if file == models::DEFAULT_VAD_MODEL {
        return Some(models::VAD_MODEL_BYTES);
    }
    models::WHISPER_MODELS
        .iter()
        .find(|m| m.file == file)
        .map(|m| m.size_bytes)
        .or_else(|| {
            models::SUMMARY_MODELS
                .iter()
                .find(|m| m.file == file && m.adopted)
                .map(|m| m.size_bytes)
        })
}
fn catalog() -> Vec<&'static str> {
    models::WHISPER_MODELS
        .iter()
        .map(|m| m.file)
        .chain(
            models::SUMMARY_MODELS
                .iter()
                .filter(|m| m.adopted)
                .map(|m| m.file),
        )
        .chain([models::TRANSLATION_MODEL_FILE, models::DEFAULT_VAD_MODEL])
        .collect()
}
fn cached_snapshot(file: &str, models_dir: &std::path::Path) -> DownloadSnapshot {
    let size = catalog_size(file).unwrap_or(0);
    let ready =
        std::fs::metadata(models_dir.join(file)).is_ok_and(|m| m.is_file() && m.len() == size);
    let partial = std::fs::metadata(models_dir.join(file).with_extension("part"))
        .map(|m| m.len().min(size))
        .unwrap_or(0);
    DownloadSnapshot {
        file: file.into(),
        size_bytes: size,
        downloaded_bytes: if ready { size } else { partial },
        status: if ready { "ready" } else { "missing" }.into(),
        error: None,
    }
}

#[tauri::command]
pub(crate) fn list_model_downloads(app: AppHandle) -> Result<Vec<DownloadSnapshot>, String> {
    let directory = crate::commands::resolve_models_dir(&app)?;
    let state = app.state::<ModelDownloadsState>();
    let transfers = state.transfers.lock().unwrap();
    Ok(catalog()
        .into_iter()
        .map(|file| {
            // Disk is authoritative once a transfer is no longer active.
            let disk = cached_snapshot(file, &directory);
            transfers
                .get(file)
                .filter(|transfer| {
                    transfer.result.borrow().is_none()
                        || (transfer.snapshot.status == "error" && disk.status != "ready")
                })
                .map(|transfer| transfer.snapshot.clone())
                .unwrap_or(disk)
        })
        .collect())
}

fn start(app: &AppHandle, file: &str) -> Result<watch::Receiver<DownloadResult>, String> {
    let size = catalog_size(file).ok_or("unknown model")?;
    let directory = crate::commands::resolve_models_dir(app)?;
    let state = app.state::<ModelDownloadsState>();
    let mut snapshot = cached_snapshot(file, &directory);
    snapshot.status = "downloading".into();
    let (receiver, sender) = state.reserve(snapshot.clone());
    let Some(sender) = sender else {
        return Ok(receiver);
    };
    let _ = app.emit("model://download", &snapshot);
    let transfers = state.transfers.clone();
    let app = app.clone();
    let file = file.to_owned();
    // The runtime owns this task even if every window or caption stops waiting.
    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let progress_transfers = transfers.clone();
        let progress_file = file.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let last = Mutex::new(Instant::now() - Duration::from_secs(1));
            let report = |done, total| {
                let mut last = last.lock().unwrap();
                if total != Some(done) && last.elapsed() < Duration::from_millis(100) {
                    return;
                }
                *last = Instant::now();
                let snapshot = {
                    let mut map = progress_transfers.lock().unwrap();
                    let transfer = map.get_mut(&progress_file).unwrap();
                    transfer.snapshot.downloaded_bytes = done;
                    transfer.snapshot.clone()
                };
                let _ = progress_app.emit("model://download", snapshot);
            };
            if progress_file == models::TRANSLATION_MODEL_FILE {
                models::ensure_translation_model(&directory, Some(&report), &|| false)
            } else {
                let url = if progress_file == models::DEFAULT_VAD_MODEL {
                    models::vad_model_url(&progress_file)
                } else if models::WHISPER_MODELS
                    .iter()
                    .any(|m| m.file == progress_file)
                {
                    models::whisper_model_url(&progress_file)
                } else {
                    models::summary_model_url(&progress_file)
                };
                models::ensure_model(&progress_file, &url, &directory, Some(&report))
            }
            .map_err(crate::commands::core_err)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Model download task failed: {e}")));
        let snapshot = {
            let mut map = transfers.lock().unwrap();
            let transfer = map.get_mut(&file).unwrap();
            match &result {
                Ok(_) => {
                    transfer.snapshot.status = "ready".into();
                    transfer.snapshot.downloaded_bytes = size;
                }
                Err(error) => {
                    transfer.snapshot.status = "error".into();
                    transfer.snapshot.error = Some(error.clone());
                }
            }
            transfer.snapshot.clone()
        };
        // Publish the final snapshot before waking waiters or allowing a retry.
        let _ = app.emit("model://download", snapshot);
        sender.send_replace(Some(result));
    });
    Ok(receiver)
}

#[tauri::command]
pub(crate) fn start_model_download(app: AppHandle, file: String) -> Result<(), String> {
    start(&app, &file)?;
    Ok(())
}

pub(crate) async fn ensure_download(
    app: &AppHandle,
    file: &str,
    cancelled: &CancellationToken,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf, String> {
    if cancelled.is_cancelled() {
        return Err("translation.cancelled".into());
    }
    let mut receiver = start(app, file)?;
    let mut interval = tokio::time::interval(Duration::from_millis(150));
    loop {
        if cancelled.is_cancelled() {
            return Err("translation.cancelled".into());
        }
        if let Some(result) = receiver.borrow().clone() {
            return result;
        }
        tokio::select! {
            _ = cancelled.cancelled() => return Err("translation.cancelled".into()),
            changed = receiver.changed() => { changed.map_err(|_| "Model download stopped".to_string())?; },
            _ = interval.tick() => {
                let state = app.state::<ModelDownloadsState>();
                let snapshot = state.transfers.lock().unwrap().get(file).map(|t| t.snapshot.clone());
                if let Some(snapshot) = snapshot { progress(snapshot.downloaded_bytes, Some(snapshot.size_bytes)); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> DownloadSnapshot {
        DownloadSnapshot {
            file: "fixture".into(),
            size_bytes: 100,
            downloaded_bytes: 40,
            status: "downloading".into(),
            error: None,
        }
    }
    #[test]
    fn leaving_every_view_does_not_release_transfer_ownership() {
        let state = ModelDownloadsState::default();
        let (first_view, worker) = state.reserve(snapshot());
        let worker = worker.expect("first request owns the worker");
        drop(first_view);
        let (second_view, duplicate_worker) = state.reserve(snapshot());
        assert!(
            duplicate_worker.is_none(),
            "navigation must join the existing worker"
        );
        assert_eq!(
            state.transfers.lock().unwrap()["fixture"]
                .snapshot
                .downloaded_bytes,
            40
        );
        worker.send_replace(Some(Ok(PathBuf::from("model"))));
        assert_eq!(
            second_view.borrow().as_ref().unwrap().as_ref().unwrap(),
            &PathBuf::from("model")
        );
    }
    #[test]
    fn failed_transfer_can_be_retried_without_reusing_its_failed_result() {
        let state = ModelDownloadsState::default();
        let (_, first) = state.reserve(snapshot());
        first.unwrap().send_replace(Some(Err("offline".into())));
        let (retry, worker) = state.reserve(snapshot());
        assert!(worker.is_some());
        assert!(retry.borrow().is_none());
    }
    #[test]
    fn download_requests_are_limited_to_the_shipped_catalog() {
        assert!(catalog_size("../../other-file").is_none());
        assert!(catalog_size("Qwen3.5-4B-Q4_K_M.gguf").is_none());
        assert_eq!(
            catalog_size(models::TRANSLATION_MODEL_FILE),
            Some(models::TRANSLATION_MODEL_BYTES)
        );
    }
}
