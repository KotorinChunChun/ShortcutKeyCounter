//! System Trayと常駐動作。

use std::thread;
use std::time::Duration;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::app_state::AppState;
use crate::capture_health::HealthLevel;
use crate::security_gate::SecurityStopReason;
use crate::window_manager::{notify_mapping_unsaved_blocked, request_show_main_window};

pub struct TrayUiState {
    capture_item: MenuItem<Wry>,
    overlay_item: CheckMenuItem<Wry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayVisualState {
    Recording,
    Paused,
    Error,
}

pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "ShortcutKeyCounter を開く", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "計測を停止", true, None::<&str>)?;
    let overlay_enabled = app
        .state::<AppState>()
        .overlay_enabled()
        .unwrap_or_else(|_| app.state::<AppState>().overlay.is_enabled());
    let overlay = CheckMenuItem::with_id(
        app,
        "overlay-toggle",
        "オーバーレイ表示",
        true,
        overlay_enabled,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &capture, &overlay, &separator, &quit])?;
    let capture_menu = capture.clone();
    let overlay_menu = overlay.clone();

    TrayIconBuilder::with_id("shortcut-key-counter-tray")
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("ShortcutKeyCounter - 計測中 / 本番DB")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => request_show_main_window(app),
            "capture" => {
                let state = app.state::<AppState>();
                if let Ok(enabled) = state.capture_enabled() {
                    if state.set_capture_enabled(!enabled).is_ok() {
                        sync_capture_ui(app, !enabled);
                    }
                }
            }
            "overlay-toggle" => {
                let state = app.state::<AppState>();
                if let Ok(current) = state.overlay_enabled() {
                    let next = !current;
                    if state.set_overlay_enabled(next).is_ok() {
                        sync_overlay_ui(app, next);
                    }
                }
            }
            "quit" => {
                let state = app.state::<AppState>();
                if state.mapping_edit_is_blocking() {
                    notify_mapping_unsaved_blocked(app);
                } else {
                    state.mark_exiting();
                    app.exit(0);
                }
            }
            _ => {}
        })
        .build(app)?;

    app.manage(TrayUiState {
        capture_item: capture_menu,
        overlay_item: overlay_menu,
    });

    // Security / exclusion / DB environment can change without a tray-menu action.
    // Poll only lightweight shared state and foreground executable; input identities are never logged here.
    let app_handle = app.handle().clone();
    let _ = thread::Builder::new()
        .name("shortcut-tray-status".to_owned())
        .spawn(move || loop {
            refresh_tray_visual(&app_handle);
            if app_handle.state::<AppState>().is_exiting() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        });
    Ok(())
}

pub fn sync_overlay_ui(app: &AppHandle, enabled: bool) {
    if let Some(state) = app.try_state::<TrayUiState>() {
        let _ = state.overlay_item.set_checked(enabled);
    }
    let _ = app.emit("overlay-enabled-changed", enabled);
}

pub fn sync_capture_ui(app: &AppHandle, enabled: bool) {
    if let Some(state) = app.try_state::<TrayUiState>() {
        let _ = state.capture_item.set_text(if enabled {
            "計測を停止"
        } else {
            "計測を再開"
        });
    }
    refresh_tray_visual(app);
    // WebViewが存在する場合は即時反映。存在しない場合は次回生成時のstatus取得で同期する。
    let _ = app.emit("capture-state-changed", enabled);
}

fn refresh_tray_visual(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("shortcut-key-counter-tray") else {
        return;
    };
    let state = app.state::<AppState>();
    let database = state.database_environment();
    let health = state.capture_health();

    let capture_enabled = state.capture_enabled().unwrap_or(false);
    let (visual, reason) = if health.level == HealthLevel::Error {
        (
            TrayVisualState::Error,
            health
                .message
                .unwrap_or_else(|| "入力取得または記録継続を保証できない障害".to_owned()),
        )
    } else if !capture_enabled {
        (TrayVisualState::Paused, "計測停止中".to_owned())
    } else if !state.capture_worker_running().unwrap_or(false) {
        (
            TrayVisualState::Error,
            "DB writer / Capture workerが停止しています".to_owned(),
        )
    } else if let Ok(security) = state.security_status.read() {
        if security.blocked {
            let text = match security.reason {
                SecurityStopReason::ProtectedInput => "保護された入力欄のため停止中",
                SecurityStopReason::HigherIntegrityForeground => "高権限アプリのため停止中",
                SecurityStopReason::Unknown => "安全性を確認できないため停止中",
                SecurityStopReason::None => "セキュリティ保護のため停止中",
            };
            (TrayVisualState::Paused, text.to_owned())
        } else if foreground_is_excluded(&state) {
            (TrayVisualState::Paused, "除外アプリのため停止中".to_owned())
        } else {
            (TrayVisualState::Recording, "計測中".to_owned())
        }
    } else {
        (TrayVisualState::Paused, "状態確認中のため停止中".to_owned())
    };

    let icon = match visual {
        TrayVisualState::Recording => tauri::include_image!("icons/32x32.png"),
        TrayVisualState::Paused => tauri::include_image!("icons/tray-paused.png"),
        TrayVisualState::Error => tauri::include_image!("icons/tray-error.png"),
    };
    let _ = tray.set_icon(Some(icon));
    let _ = tray.set_tooltip(Some(format!(
        "ShortcutKeyCounter - {reason} / {}",
        database.display_name
    )));
}

fn foreground_is_excluded(state: &AppState) -> bool {
    let Ok(executable) = shortcut_windows::foreground::foreground_executable_name() else {
        return false;
    };
    state
        .shared_exclusions
        .read()
        .map(|set| set.contains(&executable.to_ascii_lowercase()))
        .unwrap_or(true)
}
