#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::thread;

use shortcut_key_counter::app_state::AppState;
use shortcut_key_counter::tauri_commands;
use shortcut_key_counter::tray::setup_tray;
use shortcut_key_counter::window_manager::{
    destroy_main_window, notify_mapping_unsaved_blocked, request_show_main_window,
    request_show_settings_window, should_prevent_process_exit, MAIN_WINDOW_LABEL,
};
use shortcut_windows::single_instance::{
    acquire_or_notify_existing, wait_for_activation, SingleInstanceStatus,
};
use tauri::tray::{MouseButton, TrayIconEvent};
use tauri::{Emitter, Manager};

const HIDE_ARGUMENT: &str = "-hide";

fn hide_on_start(arguments: impl IntoIterator<Item = String>) -> bool {
    arguments
        .into_iter()
        .any(|argument| argument.eq_ignore_ascii_case(HIDE_ARGUMENT) || argument == "--startup")
}

fn main() {
    // Mutexをbackend初期化より前に取得する。2個目はHook/DBを二重起動せず、
    // 既存processへ「設定画面を表示」を通知して即終了する。
    let instance_guard = match acquire_or_notify_existing()
        .expect("ShortcutKeyCounter single-instance initialization failed")
    {
        SingleInstanceStatus::ExistingNotified => return,
        SingleInstanceStatus::Primary(guard) => guard,
    };
    let activation_event = instance_guard.activation_event_value();
    let start_hidden = hide_on_start(std::env::args().skip(1));
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let origin = shortcut_key_counter::management::launch_origin(&arguments).to_owned();

    let state = AppState::initialize().expect("ShortcutKeyCounter backend initialization failed");

    let app = tauri::Builder::default()
        .manage(state)
        .setup(move |app| {
            setup_tray(app)?;
            shortcut_key_counter::management::start(app.handle().clone(), origin.clone())?;

            // Capture workerがリアルタイムログへ1件追加するたびにWebViewへ通知する。
            // UIが閉じている間もログ自体はring bufferへ蓄積される。
            let realtime_receiver = app.state::<AppState>().realtime_log.subscribe();
            let realtime_app = app.handle().clone();
            let _ = thread::Builder::new()
                .name("shortcut-key-counter-realtime-events".to_owned())
                .spawn(move || loop {
                    match realtime_receiver.recv_timeout(std::time::Duration::from_millis(500)) {
                        Ok(sequence) => {
                            let _ = realtime_app.emit("realtime-log-changed", sequence);
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                    if realtime_app.state::<AppState>().is_exiting() {
                        break;
                    }
                });

            // -hideはWindowsログオン時の自動起動専用。通常起動ではUIを表示する。
            if !start_hidden {
                request_show_main_window(app.handle());
            }

            // 2個目の起動要求はnamed Eventで受け、既存processの設定画面を表示する。
            let app_handle = app.handle().clone();
            let _ = thread::Builder::new()
                .name("shortcut-single-instance-activation".to_owned())
                .spawn(move || loop {
                    if wait_for_activation(activation_event, 750) {
                        request_show_settings_window(&app_handle);
                    }
                    if app_handle.state::<AppState>().is_exiting() {
                        break;
                    }
                });

            // 永続Elevated設定の復元もsetup/main loopをblockしない。
            let elevated_app = app.handle().clone();
            let _ = thread::Builder::new()
                .name("shortcut-restore-elevated".to_owned())
                .spawn(move || {
                    let _ = elevated_app.state::<AppState>().restore_elevated_setting();
                });
            Ok(())
        })
        .on_tray_icon_event(|app, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                request_show_main_window(app);
            }
        })
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW_LABEL {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !state.is_exiting() && state.mapping_edit_is_blocking() {
                    api.prevent_close();
                    notify_mapping_unsaved_blocked(window.app_handle());
                } else if !state.is_exiting() {
                    // cleanな画面離脱ではsessionを破棄し、labels overrideも解除する。
                    if state.end_mapping_edit().is_err() {
                        api.prevent_close();
                        notify_mapping_unsaved_blocked(window.app_handle());
                    } else {
                        api.prevent_close();
                        let _ = destroy_main_window(window.app_handle());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            tauri_commands::capture::get_capture_status,
            tauri_commands::capture::set_capture_enabled,
            tauri_commands::capture::get_capture_health,
            tauri_commands::statistics::get_dashboard_summary,
            tauri_commands::statistics::get_shortcut_ranking,
            tauri_commands::statistics::get_shortcut_ranking_extended,
            tauri_commands::statistics::get_application_ranking,
            tauri_commands::statistics::get_application_statistics,
            tauri_commands::statistics::get_daily_trend,
            tauri_commands::statistics::get_hourly_trend,
            tauri_commands::statistics::get_shortcut_detail,
            tauri_commands::statistics::list_shortcuts,
            tauri_commands::statistics::get_data_bounds,
            tauri_commands::statistics::get_rankings_v014,
            tauri_commands::statistics::get_usage_graph_v014,
            tauri_commands::statistics::get_usage_graph_colors,
            tauri_commands::statistics::set_usage_graph_colors,
            tauri_commands::settings::get_excluded_apps,
            tauri_commands::settings::add_excluded_app,
            tauri_commands::settings::set_excluded_app_enabled,
            tauri_commands::settings::remove_user_excluded_app,
            tauri_commands::settings::get_settings,
            tauri_commands::settings::set_auto_start,
            tauri_commands::settings::set_theme,
            tauri_commands::settings::set_usage_tracking,
            tauri_commands::settings::set_duplicate_guard_seconds,
            tauri_commands::settings::set_detail_event_retention_days,
            tauri_commands::overlay::preview_overlay_sample,
            tauri_commands::mapping::begin_key_search,
            tauri_commands::mapping::poll_key_search,
            tauri_commands::mapping::cancel_key_search,
            tauri_commands::overlay::set_overlay_enabled,
            tauri_commands::overlay::get_overlay_settings,
            tauri_commands::overlay::preview_overlay_settings,
            tauri_commands::overlay::get_overlay_monitors,
            tauri_commands::overlay::preview_overlay_monitor,
            tauri_commands::overlay::set_overlay_settings,
            tauri_commands::overlay::reset_overlay_appearance,
            tauri_commands::overlay::get_overlay_key_labels,
            tauri_commands::overlay::set_overlay_key_labels,
            tauri_commands::overlay::set_overlay_keyboard_layout,
            tauri_commands::overlay::set_overlay_keyboard_input_mode,
            tauri_commands::mapping::list_mapping_profiles,
            tauri_commands::mapping::save_mapping_profile,
            tauri_commands::mapping::delete_mapping_profile,
            tauri_commands::mapping::rename_mapping_profile,
            tauri_commands::overlay::get_overlay_key_policy,
            tauri_commands::overlay::set_overlay_key_policy,
            tauri_commands::overlay::reload_overlay_assets,
            tauri_commands::capture::set_capture_elevated,
            tauri_commands::data::get_database_environment,
            tauri_commands::data::switch_database_environment,
            tauri_commands::data::get_sample_source_status,
            tauri_commands::data::generate_sample_data,
            tauri_commands::data::preview_delete_yesterday_and_earlier,
            tauri_commands::data::delete_yesterday_and_earlier,
            tauri_commands::data::export_daily_csv,
            tauri_commands::data::get_daily_export_filter,
            tauri_commands::data::set_daily_export_filter,
            tauri_commands::data::export_events_csv,
            tauri_commands::data::backup_database,
            tauri_commands::data::delete_all_statistics,
            tauri_commands::data::get_app_paths,
            tauri_commands::data::open_data_folder,
            tauri_commands::overlay::open_overlay_labels,
            tauri_commands::overlay::open_overlay_theme,
            tauri_commands::overlay::open_overlay_policy,
            tauri_commands::overlay::open_overlay_design_folder,
            tauri_commands::data::open_parent_folder,
            tauri_commands::mapping::get_key_mapping,
            tauri_commands::mapping::set_key_mapping,
            tauri_commands::mapping::begin_mapping_edit_session,
            tauri_commands::mapping::get_mapping_edit_session,
            tauri_commands::mapping::apply_mapping_edit_draft,
            tauri_commands::mapping::apply_mapping_edit_layout,
            tauri_commands::mapping::apply_mapping_edit_input_mode,
            tauri_commands::mapping::save_mapping_edit_session,
            tauri_commands::mapping::cancel_mapping_edit_session,
            tauri_commands::mapping::end_mapping_edit_session,
            tauri_commands::mapping::mapping_edit_session_status,
            tauri_commands::mapping::export_key_mapping_json,
            tauri_commands::app::get_realtime_logs,
            tauri_commands::app::clear_realtime_logs,
            tauri_commands::app::get_app_info,
            tauri_commands::app::open_external_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ShortcutKeyCounter");

    let exit_code = app.run_return(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<AppState>();
            let exiting = state.is_exiting();
            if !exiting && state.mapping_edit_is_blocking() {
                api.prevent_exit();
                notify_mapping_unsaved_blocked(app_handle);
                return;
            }
            if should_prevent_process_exit(exiting) {
                api.prevent_exit();
            }
        }
    });
    drop(instance_guard);
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hide_argument_suppresses_initial_ui() {
        assert!(hide_on_start(["-hide".to_owned()]));
        assert!(hide_on_start(["-HIDE".to_owned()]));
        assert!(hide_on_start(["--startup".to_owned()]));
        assert!(!hide_on_start(std::iter::empty::<String>()));
        assert!(!hide_on_start(["--background".to_owned()]));
    }
}
