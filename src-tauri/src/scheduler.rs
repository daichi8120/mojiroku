//! 会議開始スケジューラ（増分1 / ADR-0026）。
//!
//! メニューバー常駐で回る tokio タスク。カレンダー予定を定期取得し、**開始時刻を跨いだ予定**に
//! ついて OS 通知を出し、「今始まった会議」を状態として保持＋フロントへイベント発行する。
//! 録音はユーザーのクリックで始める（自動録音しない＝北極星）。検知（増分2）はここには無い。
//!
//! ⚠️ 通知は「きっかけ」。macOS はアクションボタン非対応なので、通知クリック→前面化→
//! アプリ内「録音?」プロンプト（既存 prepare 流用）で確定させる。プロンプト対象の会議は
//! この状態（[`SchedulerState::pending`]）から解決する（クリックのコールバックに依存しない）。

use crate::{commands::export::fetch_calendar_events, settings};
use chrono::{Duration as ChronoDuration, Local, NaiveDateTime};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// 予定の開始を跨いだか判定する周期。開始時刻に対して最悪この分だけ通知が遅れる。
const CHECK_INTERVAL: Duration = Duration::from_secs(30);
/// カレンダーを実取得する間隔（tick 数）。開始判定はキャッシュ照合で毎 tick 行うので、
/// ネットワーク取得は数分に一度でよい（常時ポーリングを避けバッテリ/レート制限に優しく）。
/// 6 tick × 30s = 3 分ごと。予定は数日先まで取るので 3 分のキャッシュで開始判定に不足はない。
const FETCH_EVERY_TICKS: u64 = 6;
/// 開始からこの猶予内なら「今始まった」とみなして発火する。再起動時に遠い過去の予定へ
/// 誤発火しないための窓でもある（開始が猶予より前なら無視）。
const START_GRACE_MIN: i64 = 5;
/// CalendarEvent.start / .end のフォーマット（ローカル壁時計・オフセットなし。ADR-0016）。
const WALL_FMT: &str = "%Y-%m-%dT%H:%M:%S";

/// フロントへ渡す「今始まった会議」。`meeting://starting` イベントと
/// [`get_pending_meeting`] コマンドの戻り値に使う。
#[derive(Debug, Clone, Serialize)]
pub struct StartingMeeting {
    /// CalendarEvent.id（発生ごとに一意）。
    pub id: String,
    /// 予定タイトル（録音タイトルに使う）。
    pub title: String,
    /// 開始（ローカル壁時計）。
    pub start: String,
}

/// スケジューラの共有状態。直近に発火した会議を保持し、通知クリックでウィンドウが開いた
/// フロントが [`get_pending_meeting`] で取りに来られるようにする（ライブ購読を取りこぼしても拾える）。
pub struct SchedulerState {
    pending: Mutex<Option<StartingMeeting>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }
}

/// 直近に発火した「録音を促す会議」を取得する（消さない）。フロント初期化時に呼び、
/// あればアプリ内プロンプトを出す。ユーザーが決めたら [`clear_pending_meeting`] で消す。
#[tauri::command]
pub(crate) fn get_pending_meeting(
    state: tauri::State<'_, SchedulerState>,
) -> Option<StartingMeeting> {
    state.pending.lock().unwrap().clone()
}

/// 保留中の「録音?」対象を消す（録音開始 or 却下の後にフロントが呼ぶ）。
#[tauri::command]
pub(crate) fn clear_pending_meeting(state: tauri::State<'_, SchedulerState>) {
    *state.pending.lock().unwrap() = None;
}

/// 常駐スケジューラを起動する（setup から一度だけ）。
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        run(app).await;
    });
}

async fn run(app: AppHandle) {
    // settings.json の場所（起動後は不変）。取れなければスケジューラは実質無効。
    let data_dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return,
    };
    // 通知済みイベント ID（プロセス内 dedup）。窓＋この集合で二重通知を防ぐ。
    let mut notified: HashSet<String> = HashSet::new();
    // 直近に取得した予定のキャッシュ。開始判定は毎 tick これと now を照合する。
    let mut cached: Vec<mojiroku_core::calendar::CalendarEvent> = Vec::new();
    let mut ticks: u64 = 0;

    loop {
        tokio::time::sleep(CHECK_INTERVAL).await;

        // 毎 tick で設定を読む（トグルの反映を即時に。ファイル読みは軽い）。
        let cfg = settings::load(&data_dir);
        if !cfg.auto_record_prompt {
            // OFF の間はキャッシュを持ち越さず、ON 直後（ticks=0）に必ず取り直す。
            ticks = 0;
            cached.clear();
            continue;
        }
        let lang = cfg.effective_language().to_string();

        // カレンダー取得は数分に一度。blocking（keyring/ureq）。未接続・失敗は直前のキャッシュを
        // 維持（空なら何も起きないだけ）＝一時的なオフラインで通知が消えない。
        if ticks.is_multiple_of(FETCH_EVERY_TICKS) {
            let now_fixed = Local::now().fixed_offset();
            if let Ok(Ok(ev)) =
                tauri::async_runtime::spawn_blocking(move || fetch_calendar_events(now_fixed)).await
            {
                cached = ev;
            }
        }
        ticks = ticks.wrapping_add(1);

        let now = Local::now().naive_local();
        let grace = ChronoDuration::minutes(START_GRACE_MIN);
        for ev in &cached {
            if notified.contains(&ev.id) {
                continue;
            }
            let start = match NaiveDateTime::parse_from_str(&ev.start, WALL_FMT) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if started_within(now, start, grace) {
                notified.insert(ev.id.clone());
                fire(&app, &lang, ev);
            }
        }
    }
}

/// 予定が「今始まった」窓に入っているか＝**開始済み かつ 猶予内**（開始前や猶予より古い開始は対象外）。
/// 純関数（テスト可能）。再起動時に遠い過去の予定へ誤発火しないための窓でもある。
fn started_within(now: NaiveDateTime, start: NaiveDateTime, grace: ChronoDuration) -> bool {
    start <= now && now < start + grace
}

/// 通知を出し、保留状態を更新し、フロントへ `meeting://starting` を発行する。
fn fire(app: &AppHandle, lang: &str, ev: &mojiroku_core::calendar::CalendarEvent) {
    let meeting = StartingMeeting {
        id: ev.id.clone(),
        title: ev.title.clone(),
        start: ev.start.clone(),
    };

    let (title, body) = if lang == "en" {
        (
            "Meeting started".to_string(),
            format!("\"{}\" — start recording?", ev.title),
        )
    } else {
        (
            "会議が始まりました".to_string(),
            format!("「{}」— 録音しますか？", ev.title),
        )
    };
    // granted のときだけ実表示される（未許可なら no-op）。失敗は握り潰す（アプリは動き続ける）。
    let _ = app.notification().builder().title(title).body(body).show();

    if let Some(state) = app.try_state::<SchedulerState>() {
        *state.pending.lock().unwrap() = Some(meeting.clone());
    }
    // ウィンドウが開いていればライブでプロンプトを出せるよう通知（取りこぼしは get_pending_meeting で拾う）。
    let _ = app.emit("meeting://starting", meeting);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, WALL_FMT).unwrap()
    }

    #[test]
    fn fires_only_when_started_and_within_grace() {
        let now = dt("2026-07-18T10:03:00");
        let grace = ChronoDuration::minutes(START_GRACE_MIN); // 5 分

        // ちょうど開始・開始 3 分後（猶予内）→ 発火。
        assert!(started_within(now, dt("2026-07-18T10:03:00"), grace));
        assert!(started_within(now, dt("2026-07-18T10:00:00"), grace));
        // まだ開始前（2 分後に始まる）→ 発火しない。
        assert!(!started_within(now, dt("2026-07-18T10:05:00"), grace));
        // 開始から猶予を過ぎた（6 分前に始まった）→ 発火しない（再起動時の過去予定への誤発火防止）。
        assert!(!started_within(now, dt("2026-07-18T09:57:00"), grace));
        // 猶予境界（ちょうど 5 分前）は排他 → 発火しない。
        assert!(!started_within(now, dt("2026-07-18T09:58:00"), grace));
    }
}
