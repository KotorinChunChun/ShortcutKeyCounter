//! UI WebViewの遅延生成と完全破棄を管理する。
//!
//! `-hide` 起動時だけWebViewを作らない。通常起動はUIを表示する。
//! 閉じたWebViewはhideせずdestroyし、次回表示時に再生成する。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

pub const MAIN_WINDOW_LABEL: &str = "main";
static SHOW_REQUEST_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static PENDING_PAGE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn pending_page() -> &'static Mutex<Option<String>> {
    PENDING_PAGE.get_or_init(|| Mutex::new(None))
}

pub fn request_show_main_window(app: &AppHandle<Wry>) {
    request_show_main_window_page(app, None);
}

pub fn request_show_settings_window(app: &AppHandle<Wry>) {
    request_show_main_window_page(app, Some("settings"));
}

pub fn request_show_key_mapping_window(app: &AppHandle<Wry>) {
    request_show_main_window_page(app, Some("key-mapping"));
}

pub fn notify_mapping_unsaved_blocked(app: &AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit_to(MAIN_WINDOW_LABEL, "mapping-unsaved-blocked", ());
    } else {
        request_show_key_mapping_window(app);
    }
}

fn request_show_main_window_page(app: &AppHandle<Wry>, page: Option<&str>) {
    if let Some(page) = page {
        if let Ok(mut pending) = pending_page().lock() {
            *pending = Some(page.to_owned());
        }
    }

    if SHOW_REQUEST_IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let app = app.clone();
    let spawn = thread::Builder::new()
        .name("shortcut-ui-create".to_owned())
        .spawn(move || {
            loop {
                let page = pending_page().lock().ok().and_then(|mut page| page.take());
                let _ = show_main_window(&app, page.as_deref());
                let has_pending_page = pending_page()
                    .lock()
                    .map(|page| page.is_some())
                    .unwrap_or(false);
                if !has_pending_page {
                    break;
                }
            }
            SHOW_REQUEST_IN_FLIGHT.store(false, Ordering::Release);
            if pending_page()
                .lock()
                .map(|page| page.is_some())
                .unwrap_or(false)
            {
                request_show_main_window_page(&app, None);
            }
        });

    if spawn.is_err() {
        SHOW_REQUEST_IN_FLIGHT.store(false, Ordering::Release);
    }
}

/// event handler外から呼ぶ実際の生成処理。
pub fn show_main_window(app: &AppHandle<Wry>, page: Option<&str>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.show()?;
        let _ = window.unminimize();
        window.set_focus()?;
        if let Some(page) = page {
            let _ = app.emit_to(MAIN_WINDOW_LABEL, "navigate-page", page);
        }
        return Ok(());
    }

    let mut builder =
        WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
            .title("ShortcutKeyCounter")
            .inner_size(1220.0, 800.0)
            .min_inner_size(940.0, 640.0)
            .resizable(true)
            .center();

    if let Some(page) = page {
        let page_json = serde_json::to_string(page).unwrap_or_else(|_| "\"dashboard\"".to_owned());
        builder = builder.initialization_script(format!(
            "window.__SHORTCUT_KEY_COUNTER_INITIAL_PAGE__ = {page_json};"
        ));
    }

    let window = builder.build()?;
    window.set_focus()?;
    Ok(())
}

/// ユーザーが閉じたときは非表示化せずnative window + WebViewを破棄する。
pub fn destroy_main_window(app: &AppHandle<Wry>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy()?;
    }
    Ok(())
}

pub fn should_prevent_process_exit(exiting_by_user: bool) -> bool {
    !exiting_by_user
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_last_webview_keeps_background_process_alive() {
        assert!(should_prevent_process_exit(false));
        assert!(!should_prevent_process_exit(true));
    }
}
