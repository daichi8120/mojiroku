//! 文字起こし・要約コマンド（File 経路 / ローカル sidecar 要約 / クラウド BYOK 要約）。

use super::*;
use crate::jobs::JobQueue;
use crate::settings;
use tauri::State;

/// 土台の連結確認用コマンド。frontend の invoke('health') → ここ → mojiroku-core。
#[tauri::command]
pub(crate) fn health() -> String {
    mojiroku_core::health()
}

/// 音声ファイル → 文字起こしジョブを投入（ADR-0024 非同期フリップ）。
/// 原本を `recordings/<id>.<ext>` へ確定コピー（**ワーカーの STT 入力になる正本**）してから
/// 録音行（transcript 無し）を作りジョブを積み、**即座に返す**。STT/話者分離はワーカーが 1 本ずつ回す。
/// 呼び出し側は `recording_id` で DetailView へ遷移し、`job://update` で進捗を追う。
/// `record_only=true`（増分5「音声だけ保存」）は録音行だけ作ってジョブは積まず、後で処理する。
#[tauri::command]
pub(crate) async fn transcribe_file(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    queue: State<'_, JobQueue>,
    path: String,
    diarize: bool,
    record_only: bool,
) -> Result<StartJobResult, String> {
    let id = uuid::Uuid::new_v4().to_string();

    // 原本を recordings/<id>.<ext> へ複製する。フリップ後はワーカーがこのコピーを STT で読むため、
    // **成功は必須**（旧 best-effort から致命へ格上げ）。失敗したら録音行もジョブも作らず即エラー
    // ＝ orphan（音声の無い doomed 録音）を残さない。再エンコードせず拡張子を保持
    // （2h の m4a を WAV 化すると肥大するため）。
    copy_recording_original(&app, &id, &path).await?;

    // duration は enqueue 時 0（暫定）。ジョブ完了の replace_transcript で最終 segment 末尾へ確定。
    let title = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    let recording = mojiroku_core::Recording {
        id: id.clone(),
        source_type: mojiroku_core::SourceType::File,
        title,
        duration_ms: 0,
        sample_rate: mojiroku_core::audio::WHISPER_SAMPLE_RATE,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    insert_recording_and_maybe_enqueue(&app, &store, &queue, &recording, diarize, record_only)
}

/// 文字起こし結果 → 要約/議事録（Phase 1b・ローカル既定）。
/// ggml 衝突回避のため、ローカル要約は別バイナリ `mojiroku-llm`（sidecar）で実行する（ADR-0007）。
/// 本体は LLM モデルを確保し、プロンプトを temp に書いて sidecar に渡し、stdout を受け取る。
#[tauri::command]
pub(crate) async fn summarize(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    transcript: mojiroku_core::Transcript,
    recording_id: String,
    template_id: String,
) -> Result<mojiroku_core::Summary, String> {
    // 要約エンジン設定を読む。cloud（BYOK）ならクラウド経路へ分岐する。
    // 以降のローカル sidecar 経路は従来どおり（純加算ブランチ＝ローカル経路は不変）。
    let cfg = load_settings(&app)?;
    // 要約の出力言語（テンプレ instruction・プロンプトのマーカー・sidecar/BYOK の
    // システムプロンプトに反映）。未設定の旧 settings.json は ja。
    let lang = mojiroku_core::lang::Lang::from_code(cfg.effective_language());
    if cfg.engine == "cloud" {
        let summary = summarize_cloud(&app, transcript, template_id, &cfg).await?;
        store
            .save_summary(&recording_id, &summary)
            .map_err(|e| e.to_string())?;
        return Ok(summary);
    }

    use tauri_plugin_shell::ShellExt;

    let models_dir = resolve_models_dir(&app)?;

    // 重い ML ジョブを直列化: ローカル要約 sidecar は 4.4GB モデルを積むため、
    // 文字起こし/話者分離と同時に走らせない（メモリ枯渇によるクラッシュ/フリーズ予防）。
    // permit は sidecar 完了まで保持する。
    let _heavy_permit = acquire_heavy_job(&app, "summarize://progress").await;

    // 1) LLM モデル確保（必要なら DL, blocking）+ プロンプト構築
    let app_dl = app.clone();
    let template_id2 = template_id.clone();
    let (model_path, prompt) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(std::path::PathBuf, String), String> {
            let dl_cb = |done: u64, total: Option<u64>| {
                emit_progress(&app_dl, "summarize://progress", "download_llm", done, total)
            };
            let model_path = mojiroku_core::models::ensure_model(
                mojiroku_core::models::DEFAULT_SUMMARY_MODEL,
                &mojiroku_core::models::summary_model_url(mojiroku_core::models::DEFAULT_SUMMARY_MODEL),
                &models_dir,
                Some(&dl_cb),
            )
            .map_err(core_err)?;
            let template = mojiroku_core::summarize::template_by_id(&template_id2, lang);
            let prompt = mojiroku_core::summarize::build_prompt(&transcript, &template, lang);
            Ok((model_path, prompt))
        },
    )
    .await
    .map_err(|e| e.to_string())??;

    // 2) プロンプトを temp ファイルへ（巨大な文字起こしを引数で渡さない）
    let prompt_file = std::env::temp_dir().join(format!("mojiroku-prompt-{}.txt", std::process::id()));
    std::fs::write(&prompt_file, &prompt).map_err(|e| e.to_string())?;

    emit_progress(&app, "summarize://progress", "summarize", 0, None);

    // 3) sidecar 実行（externalBin。ADR-0007）。--lang でシステムプロンプト等の言語を揃える。
    let result = app
        .shell()
        .sidecar("mojiroku-llm")
        .map_err(|e| e.to_string())?
        .args([
            model_path.to_string_lossy().to_string(),
            prompt_file.to_string_lossy().to_string(),
            "--lang".to_string(),
            lang.code().to_string(),
        ])
        .output()
        .await
        .map_err(|e| e.to_string());

    let _ = std::fs::remove_file(&prompt_file);
    let output = result?;

    if !output.status.success() {
        return Err(format!(
            "error.summarize.sidecar_failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let summary = mojiroku_core::Summary {
        template_id,
        content,
        action_items: Vec::new(),
        stale: false,
    };
    // 同一 recording に要約を追加保存（save_summary は同期メソッドで .await を跨がない）。
    store
        .save_summary(&recording_id, &summary)
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

/// クラウド（BYOK）要約。鍵はキーチェーンから Rust 内で直接取得し、webview へ往復させない。
/// ⚠️ BYOK 利用時はデータが端末外（各プロバイダ）へ送信される（プライバシーのトレードオフ）。
async fn summarize_cloud(
    app: &AppHandle,
    transcript: mojiroku_core::Transcript,
    template_id: String,
    cfg: &settings::Settings,
) -> Result<mojiroku_core::Summary, String> {
    use mojiroku_core::summarize::{
        template_by_id, AnthropicSummarizer, OpenAiSummarizer, SummarizeProvider,
    };

    let model = cfg.effective_model(); // 空文字を API に送らない（既定へ解決済み）
    let provider = cfg.provider.clone();
    let key_name = secrets::byok_key_name(&provider);
    // 要約の出力言語（テンプレ instruction・システムプロンプトに反映）。
    let lang = mojiroku_core::lang::Lang::from_code(cfg.effective_language());

    emit_progress(app, "summarize://progress", "summarize", 0, None);

    // キーチェーン取得（許可ダイアログでブロックし得る）と ureq はどちらも blocking。
    // tokio ワーカーをブロックしないよう spawn_blocking で一括して回す。
    tauri::async_runtime::spawn_blocking(
        move || -> Result<mojiroku_core::Summary, String> {
            let api_key = get_secret_or_error(&key_name, "error.summarize.api_key_missing")?;
            let template = template_by_id(&template_id, lang);
            let result = match provider.as_str() {
                "openai" => {
                    OpenAiSummarizer { api_key, model, lang }.summarize(&transcript, &template)
                }
                _ => AnthropicSummarizer { api_key, model, lang }.summarize(&transcript, &template),
            };
            result.map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|e| e.to_string())?
}
