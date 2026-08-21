//! メニューバートレイ常駐（増分1 / ADR-0026）。
//!
//! 会議開始の自動録音プロンプトを出すには、mojiroku をバックグラウンドに常駐させて
//! 予定の開始時刻を監視する必要がある。その土台としてトレイアイコン・メニュー・通知権限
//! 要求をここに置く。スケジューラ本体（カレンダー監視→通知発火）は後続タスクで別モジュール化する。
//!
//! ⚠️ macOS はアクションボタン付き通知を非対応（Actions API はモバイル専用）。よって通知は
//! プレーンで、クリック→アプリ前面化→アプリ内「録音?」プロンプトという導線で確定させる（ADR-0026）。
//! 本モジュールにはその裏取り用の「テスト通知」メニュー項目を暫定で置いている。

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime,
};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// メインウィンドウを前面に出す（最小化/非表示からの復帰も含む）。
/// 通知クリック後の想定導線でもある（macOS では通知クリックがアプリを activate する）。
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 通知権限（macOS の TCC）が未確定なら要求する。best-effort＝拒否されてもアプリは通常どおり
/// 動く（通知が出ないだけ）。`show()` は granted のときのみ実際に表示される。
fn ensure_notification_permission<R: Runtime>(app: &AppHandle<R>) {
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => {}
        Ok(_) => {
            let _ = app.notification().request_permission();
        }
        Err(_) => {}
    }
}

/// トレイを構築し setup から一度だけ呼ぶ。
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    ensure_notification_permission(app);

    let show_i = MenuItem::with_id(app, "show", "mojiroku を表示", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("mojiroku")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        });

    // トレイアイコンはアプリの既定ウィンドウアイコンを流用（macOS メニューバー向けの
    // テンプレート化は後続で調整）。アイコンが無ければアイコンなしで作る。
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}
