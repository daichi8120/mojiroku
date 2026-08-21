//! 外部サービス連携コマンド（Notion / Slack エクスポート、カレンダー取込、OAuth 連携）。

use super::*;
use crate::secrets::{CALENDAR_ICAL_KEY, NOTION_TOKEN_KEY, SLACK_WEBHOOK_KEY};
use crate::oauth;
use tauri::State;

/// Notion へ議事録ページを送信する（内部インテグレーション トークン = BYOK, $0）。
/// トークンはキーチェーン（[`NOTION_TOKEN_KEY`]）、親ページ ID は settings.json（`notion_parent_id`）。
/// ⚠️ 要約 + 文字起こしが Notion のサーバへ送られる（ローカル要約でも送信される）。
/// 呼び出しはユーザー操作（ポップオーバーのボタン）起点のみ。戻り値は作成された Notion ページの URL。
#[tauri::command]
pub(crate) async fn export_to_notion(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    recording_id: String,
) -> Result<String, String> {
    // 送信データは State を持ち込まずに済むよう、ここで owned な RecordingDetail を取り出す。
    let detail = resolve_recording_detail(&store, &recording_id)?;
    let cfg = load_settings(&app)?;
    let parent_id = cfg.notion_parent_id.trim().to_string();
    if parent_id.is_empty() {
        return Err("error.export.notion_parent_missing".to_string());
    }

    // 見出し・既定文言の言語はエクスポート実行時のアプリ設定に追従する。
    let lang = mojiroku_core::lang::Lang::from_code(cfg.effective_language());

    // キーチェーン取得（許可ダイアログでブロックし得る）と ureq はどちらも blocking → spawn_blocking。
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let token = get_secret_or_error(NOTION_TOKEN_KEY, "error.export.notion_not_connected")?;
        mojiroku_core::export::NotionExporter { token, parent_id, lang }
            .export(&detail)
            .map_err(core_err)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Slack へ要約を投稿する（Incoming Webhook = BYOK, $0, OAuth 不要）。
/// webhook URL はキーチェーン（[`SLACK_WEBHOOK_KEY`]）。チャンネルは URL に内包（設定不要）。
/// ⚠️ **要約**が Slack のサーバへ送られる（Notion と違い文字起こしは送らない）。
/// 呼び出しはユーザー操作（ポップオーバーのボタン）起点のみ。
#[tauri::command]
pub(crate) async fn export_to_slack(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    recording_id: String,
) -> Result<(), String> {
    // 送信データは State を持ち込まずに済むよう、ここで owned な RecordingDetail を取り出す。
    let detail = resolve_recording_detail(&store, &recording_id)?;

    // ラベル・既定タイトルの言語はエクスポート実行時のアプリ設定に追従する。
    let lang = mojiroku_core::lang::Lang::from_code(load_settings(&app)?.effective_language());

    // キーチェーン取得と ureq はどちらも blocking → spawn_blocking。
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let webhook = get_secret_or_error(SLACK_WEBHOOK_KEY, "error.export.slack_not_connected")?;
        mojiroku_core::export::SlackExporter { webhook_url: webhook, lang }
            .export(&detail)
            .map_err(core_err)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Google カレンダーの限定公開 iCal フィードから直近の予定を取り込む（読み取り専用・$0・OAuth 不要）。
/// URL はキーチェーン（[`CALENDAR_ICAL_KEY`]）。⚠️ こちらへ送信するものは無く、この画面の表示時に
/// 我々から basic.ics を GET して解析するだけ。`now` はここで採取し core へ注入する（core はテスト決定的）。
#[tauri::command]
pub(crate) async fn list_calendar_events() -> Result<Vec<mojiroku_core::calendar::CalendarEvent>, String> {
    let now = chrono::Local::now().fixed_offset();
    // キーチェーン取得と ureq はどちらも blocking → spawn_blocking。
    tauri::async_runtime::spawn_blocking(move || fetch_calendar_events(now))
        .await
        .map_err(|e| e.to_string())?
}

/// カレンダー予定を取得する blocking ヘルパー。連携画面（[`list_calendar_events`]）と
/// 会議開始スケジューラ（[`crate::scheduler`], ADR-0026）で共用する。OAuth 連携済みなら
/// Calendar API を優先し、未連携なら従来の限定公開 iCal にフォールバック（既存ユーザーを壊さない）。
/// `now` は呼び出し側で採取して注入する（core はテスト決定的）。keyring/ureq とも blocking。
pub(crate) fn fetch_calendar_events(
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<Vec<mojiroku_core::calendar::CalendarEvent>, String> {
    use mojiroku_core::calendar::{DEFAULT_MAX_EVENTS, DEFAULT_WINDOW_DAYS};
    if secrets::get(oauth::GOOGLE_REFRESH_KEY)?
        .filter(|s| !s.trim().is_empty())
        .is_some()
    {
        let token = oauth::valid_google_access_token()?;
        return mojiroku_core::calendar::fetch_calendar_api(
            &token,
            now,
            DEFAULT_WINDOW_DAYS,
            DEFAULT_MAX_EVENTS,
        )
        .map_err(core_err);
    }
    let url = get_secret_or_error(CALENDAR_ICAL_KEY, "error.calendar.not_connected")?;
    mojiroku_core::calendar::CalendarFeed { ical_url: url }
        .fetch_upcoming(now, DEFAULT_WINDOW_DAYS, DEFAULT_MAX_EVENTS)
        .map_err(core_err)
}

/// 外部サービスと OAuth 連携する（loopback + PKCE, $0・client_secret なし）。provider 別に分岐。
/// Slack: `incoming-webhook` で得た Webhook URL を Keychain（[`SLACK_WEBHOOK_KEY`]）へ保存し、
/// 既存の `export_to_slack`（SlackExporter）がそのまま使える。Google/Notion は順次追加。
#[tauri::command]
pub(crate) async fn oauth_connect(app: AppHandle, provider: String) -> Result<(), String> {
    match provider.as_str() {
        "slack" => oauth::connect_slack(app).await,
        "google" => oauth::connect_google(app).await,
        "notion" => oauth::connect_notion(app).await,
        other => Err(format!("未対応の連携プロバイダ: {other}")),
    }
}

/// 連携トークンでアクセスできる Notion ページ（書き出し先候補）を返す。OAuth 同意でユーザーが
/// 共有を許可したページが出る。フロントの「書き出し先ページ」ドロップダウン用。値は返すが
/// トークン自体は返さない。`(async)` でキーチェーン取得 + ureq をメインスレッド外へ。
#[tauri::command(async)]
pub(crate) fn notion_accessible_pages() -> Result<Vec<mojiroku_core::export::NotionPage>, String> {
    let token = get_secret_or_error(NOTION_TOKEN_KEY, "error.export.notion_not_connected")?;
    mojiroku_core::export::notion_accessible_pages(&token).map_err(core_err)
}
