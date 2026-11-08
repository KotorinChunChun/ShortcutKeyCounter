//! Realtime Log、アプリ情報、外部URL。

use tauri::State;

use crate::app_info::{self, AppInfo};
use crate::app_state::AppState;
use crate::command_error::CommandError;
use crate::realtime_log::RealtimeLogEntry;

#[tauri::command]
pub fn get_realtime_logs(
    state: State<'_, AppState>,
    after_sequence: u64,
    limit: usize,
) -> Vec<RealtimeLogEntry> {
    state.realtime_log.entries_after(after_sequence, limit)
}

#[tauri::command]
pub fn clear_realtime_logs(state: State<'_, AppState>) {
    state.realtime_log.clear();
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    app_info::current()
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), CommandError> {
    if !url.starts_with("https://github.com/") && url != "https://github.com/" {
        return Err(CommandError::new(
            "INVALID_URL",
            "許可されていないURLです。",
        ));
    }
    shortcut_windows::shell_open::open_url(&url)
        .map_err(|_| CommandError::new("OPEN_URL_FAILED", "GitHubを開けませんでした。"))
}
