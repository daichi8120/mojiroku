//! Google カレンダーの「限定公開 iCal URL（Secret address in iCal format）」から
//! 直近の予定を取り込む（読み取り専用・$0・OAuth 不要）。ADR-0016。
//!
//! 方式は Slack(Incoming Webhook) と同じ **秘密 URL を貼るだけ**。URL
//! （`https://calendar.google.com/calendar/ical/<id>/private-<token>/basic.ics`）自体が
//! カレンダー読み取りのクレデンシャルなので、キーチェーン管轄（src-tauri の `secrets`）に置き、
//! ⚠️ エラー文字列にも URL/トークンを一切含めない（slack.rs の教訓を一般化）。
//!
//! ## 構成
//! - 本モジュール（`mod.rs`）: **fetch と公開 API**。iCal の HTTP 取得（`fetch_ics` / `CalendarFeed`）、
//!   Calendar API v3 経路（`fetch_calendar_api`）、公開型（`CalendarEvent`）を持つ。
//! - `ical` サブモジュール: **iCal 本文の解析と RRULE 展開**（純粋層・ネットワーク I/O 無し）。
//!
//! ## 設計（裏取り済み）
//! - Google の `basic.ics` は**繰り返し予定を単一 VEVENT + RRULE で返す**（展開しない。EXDATE /
//!   RECURRENCE-ID 付き）。よって「週次定例」を「次の予定」に出すには RRULE 展開が要る。
//! - 展開は **NaiveDateTime（壁時計）**で行う。RRULE の意味論はイベントのローカル壁時計で定義され
//!   （「毎週火 15:00」は DST に関係なく 15:00）、Naive 加算が最も単純かつ DST ドリフトを生まない。
//! - 絶対時刻の比較・並べ替えは単一フレーム（ローカル壁時計）へ正規化してから行う。`...Z`(UTC) は
//!   呼び出し側が注入する `now` のオフセットでローカルへ変換する（テスト決定的・`clock` 不要）。
//!
//! ## MVP の制限（UI で開示）
//! - **対応 FREQ は DAILY / WEEKLY のみ**（+INTERVAL/BYDAY/COUNT/UNTIL/EXDATE）。MONTHLY/YEARLY は
//!   マスタの 1 回だけ（窓内に無ければ表示されない）。
//! - **TZID は壁時計をそのままローカル扱い**（同一 TZ では正しい。異 TZ の予定は時刻がずれ得る）。
//! - **全日予定（VALUE=DATE）は一覧から除外**（録音対象の「会議」ではないため）。
//! - Google 側エッジキャッシュで**直近の変更反映に多少のラグ**があり得る（これは UI で開示）。

use crate::error::{CoreError, Result};
use chrono::{DateTime, Duration, FixedOffset};
use serde::Serialize;
use std::io::Read;

/// iCal 本文の解析と RRULE 展開（純粋層・ネットワーク I/O 無し）。
mod ical;

/// 限定公開 iCal URL の必須プレフィックス（誤 URL で秘密を他ホストへ送らないためのガード）。
const ICAL_URL_PREFIX: &str = "https://calendar.google.com/calendar/ical/";
/// HTTP 全体タイムアウト（秒）。応答ストールでスレッドが無期限ブロックしない保険。
const HTTP_TIMEOUT_SECS: u64 = 30;
/// レスポンス読み込みの上限バイト（巨大フィードでの OOM 保険）。basic.ics は通常〜数 MB。
const MAX_FEED_BYTES: u64 = 24 * 1024 * 1024;
/// フロントへ返す壁時計フォーマット（オフセットなし `YYYY-MM-DDTHH:MM:SS`）。Calendar-API 経路
/// （to_local_wall）と iCal 経路（`ical::parse_and_expand`、`super::WALL_FMT` で参照）で同形式である
/// ことが `new Date` 解釈の前提なので、両者で必ずこの定数を共有する。id/parse 用のコンパクト形式
/// "%Y%m%dT%H%M%S" とは別物。
const WALL_FMT: &str = "%Y-%m-%dT%H:%M:%S";

/// 既定の表示窓（日）。今日から何日先までの予定を出すか。
pub const DEFAULT_WINDOW_DAYS: i64 = 14;
/// 既定の最大件数。
pub const DEFAULT_MAX_EVENTS: usize = 20;

/// フロントへ返す 1 予定（発生単位。繰り返しは発生ごとに 1 件）。
/// ⚠️ 秘密 URL に由来する情報は一切含めない。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarEvent {
    /// React key 用の安定 ID（UID + 発生開始）。繰り返しでも発生ごとに一意。
    pub id: String,
    /// 予定タイトル（SUMMARY をアンエスケープ済み）。
    pub title: String,
    /// 開始（ローカル壁時計の `YYYY-MM-DDTHH:MM:SS`、オフセットなし＝JS の `new Date` がローカル解釈）。
    pub start: String,
    /// 終了（同形式）。元データに DTEND が無ければ `None`。
    pub end: Option<String>,
    /// 場所（LOCATION）。無ければ `None`。
    pub location: Option<String>,
}

/// 限定公開 iCal URL から予定を取り込むフェッチャ（URL = 秘密、キーチェーン由来）。
pub struct CalendarFeed {
    /// 限定公開 iCal URL（`https://calendar.google.com/calendar/ical/…/basic.ics`）。
    pub ical_url: String,
}

impl CalendarFeed {
    /// `now` から `window_days` 先までの予定を最大 `max` 件、開始の昇順で返す。
    /// `now` は呼び出し側が注入（`chrono::Local::now().fixed_offset()`）→ テスト決定的。
    pub fn fetch_upcoming(
        &self,
        now: DateTime<FixedOffset>,
        window_days: i64,
        max: usize,
    ) -> Result<Vec<CalendarEvent>> {
        let text = fetch_ics(&self.ical_url)?;
        Ok(ical::parse_and_expand(&text, now, window_days, max))
    }
}

/// OAuth（Calendar API v3 `events.list`）で直近の予定を取り込む。`access_token` は有効なものを
/// 呼び出し側（src-tauri の OAuth トークン管理）が用意する。`singleEvents=true` で繰り返しは
/// **サーバ側展開**（iCal 経路の RRULE 展開が不要）・`orderBy=startTime` で昇順。全日予定
/// （start.date のみ）は録音対象でないため除外。⚠️ token はヘッダで送り、エラーに含めない。
pub fn fetch_calendar_api(
    access_token: &str,
    now: DateTime<FixedOffset>,
    window_days: i64,
    max: usize,
) -> Result<Vec<CalendarEvent>> {
    let time_min = now.to_rfc3339();
    let time_max = (now + Duration::days(window_days)).to_rfc3339();
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build();
    let resp = agent
        .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .query("timeMin", &time_min)
        .query("timeMax", &time_max)
        .query("singleEvents", "true")
        .query("orderBy", "startTime")
        .query("maxResults", &max.to_string())
        .set("Authorization", &format!("Bearer {access_token}"))
        .call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => {
            return Err(CoreError::Calendar(
                "Google の認証が切れました。再連携してください。".to_string(),
            ))
        }
        Err(_) => {
            return Err(CoreError::Calendar(
                "Google カレンダーの取得に失敗しました".to_string(),
            ))
        }
    };
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|_| CoreError::Calendar("Calendar API 応答の解析に失敗しました".to_string()))?;

    let mut out = Vec::new();
    let items = json.get("items").and_then(|v| v.as_array());
    for item in items.into_iter().flatten() {
        // 全日予定（start.date のみ）は除外。時間指定（start.dateTime）だけ採用。
        let start_raw = match item.pointer("/start/dateTime").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        let start = to_local_wall(start_raw, now)?;
        let end = item
            .pointer("/end/dateTime")
            .and_then(|v| v.as_str())
            .and_then(|s| to_local_wall(s, now).ok());
        let title = item
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("(無題)")
            .to_string();
        let location = item
            .get("location")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| start.clone());
        out.push(CalendarEvent { id, title, start, end, location });
        if out.len() >= max {
            break;
        }
    }
    Ok(out)
}

/// RFC3339 の日時を、`now` のローカルオフセットの壁時計 `YYYY-MM-DDTHH:MM:SS`（オフセットなし）に
/// する。フロント（iCal 経路と同形式）は `new Date` でローカル解釈する。
fn to_local_wall(rfc3339: &str, now: DateTime<FixedOffset>) -> Result<String> {
    let dt = DateTime::parse_from_rfc3339(rfc3339)
        .map_err(|_| CoreError::Calendar("予定日時の解析に失敗しました".to_string()))?;
    let local = dt.with_timezone(now.offset());
    Ok(local.naive_local().format(WALL_FMT).to_string())
}

/// iCal を GET して本文を返す。URL を検証し、エラーには URL/トークンを含めない。
fn fetch_ics(url: &str) -> Result<String> {
    let url = validate_url(url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build();
    let resp = agent.get(&url).call().map_err(cal_err)?;
    let mut body = String::new();
    resp.into_reader()
        .take(MAX_FEED_BYTES)
        .read_to_string(&mut body)
        .map_err(|_| CoreError::Calendar("iCal の読み込みに失敗しました".to_string()))?;
    // HTML エラーページ等を予定 0 件と誤認しないよう、iCal であることを確認する。
    if !body.contains("BEGIN:VCALENDAR") {
        return Err(CoreError::Calendar(
            "iCal フィードとして解釈できませんでした（URL を再取得してください）".to_string(),
        ));
    }
    Ok(body)
}

/// URL を検証して trim 済みを返す。Google の限定公開 iCal プレフィックスで始まらなければ拒否。
/// ⚠️ エラーに入力（＝秘密）をエコーしない。
fn validate_url(raw: &str) -> Result<String> {
    let s = raw.trim();
    if !s.starts_with(ICAL_URL_PREFIX) {
        return Err(CoreError::Calendar(format!(
            "iCal URL の形式が不正です（{ICAL_URL_PREFIX}… で始まる限定公開 URL を貼ってください）。設定はカレンダーの「設定 → カレンダーを統合 → iCal 形式の限定公開 URL」。"
        )));
    }
    Ok(s.to_string())
}

/// HTTP エラーを人間可読に。⚠️ ボディ/URL は出さない（秘密の token を含む path がエラー HTML に
/// 載り得るため）。ステータスのヒントのみ。Transport は `kind()`（URL 非依存）のみ。
fn cal_err(e: ureq::Error) -> CoreError {
    match e {
        ureq::Error::Status(code, _resp) => {
            let hint = match code {
                401 | 403 => "（URL が無効か限定公開設定が変更された可能性。URL を再取得してください）",
                404 => "（カレンダーが見つかりません。URL を再取得してください）",
                _ => "",
            };
            CoreError::Calendar(format!("iCal 取得に失敗しました {code}{hint}"))
        }
        other => CoreError::Calendar(format!("iCal 取得 通信エラー: {}", other.kind())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_rejects_non_google() {
        assert!(validate_url("https://evil.example.com/x.ics").is_err());
        assert!(validate_url("  ").is_err());
        assert!(validate_url(
            "https://calendar.google.com/calendar/ical/me%40x.com/private-abc/basic.ics"
        )
        .is_ok());
    }
}
