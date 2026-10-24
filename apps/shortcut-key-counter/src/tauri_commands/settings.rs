//! 一般設定と除外アプリ設定。

use shortcut_storage::repositories::excluded_app_repository::ExcludedApp;
use shortcut_storage::StorageError;
use tauri::State;

use super::{read_settings, settings_db_lock, settings_snapshot};
use crate::app_state::AppState;
use crate::command_api::{self, ExclusionCommandError, SettingsSnapshot};
use crate::command_error::CommandError;

#[tauri::command]
pub fn get_excluded_apps(state: State<'_, AppState>) -> Result<Vec<ExcludedApp>, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::get_excluded_apps(&db).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn add_excluded_app(
    state: State<'_, AppState>,
    executable_name: String,
) -> Result<Vec<ExcludedApp>, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::add_excluded_app_and_reload(&db, &state.shared_exclusions, &executable_name)
        .map_err(|error| add_excluded_app_error(&error))?;
    command_api::get_excluded_apps(&db).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn set_excluded_app_enabled(
    state: State<'_, AppState>,
    executable_name: String,
    enabled: bool,
) -> Result<Vec<ExcludedApp>, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::set_excluded_app_enabled_and_reload(
        &db,
        &state.shared_exclusions,
        &executable_name,
        enabled,
    )
    .map_err(|_| CommandError::new("SETTINGS_FAILED", "除外設定を変更できませんでした。"))?;
    command_api::get_excluded_apps(&db).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn remove_user_excluded_app(
    state: State<'_, AppState>,
    executable_name: String,
) -> Result<Vec<ExcludedApp>, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::remove_user_excluded_app_and_reload(
        &db,
        &state.shared_exclusions,
        &executable_name,
    )
    .map_err(|_| CommandError::new("SETTINGS_FAILED", "除外アプリを削除できませんでした。"))?;
    command_api::get_excluded_apps(&db).map_err(|_| CommandError::db())
}

fn add_excluded_app_error(error: &ExclusionCommandError) -> CommandError {
    let code = match error {
        ExclusionCommandError::Storage(StorageError::InvalidExecutableName) => {
            "INVALID_EXECUTABLE_NAME"
        }
        _ => "SETTINGS_FAILED",
    };
    CommandError::new(code, "除外アプリを追加できませんでした。")
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsSnapshot, CommandError> {
    settings_snapshot(&state)
}

#[tauri::command]
pub fn set_auto_start(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    let executable = std::env::current_exe().map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "実行ファイルの場所を取得できませんでした。",
        )
    })?;
    let db = settings_db_lock(&state)?;
    command_api::set_auto_start_command(&db, executable, enabled).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "自動起動設定を変更できませんでした。")
    })?;
    read_settings(&db)
}

#[tauri::command]
pub fn set_theme(
    state: State<'_, AppState>,
    theme: String,
) -> Result<SettingsSnapshot, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::set_theme_command(&db, &theme)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "テーマ設定を保存できませんでした。"))?;
    read_settings(&db)
}

#[tauri::command]
pub fn set_usage_tracking(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_usage_tracking(enabled).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "利用時間設定を保存できませんでした。")
    })?;
    settings_snapshot(&state)
}

#[tauri::command]
pub fn set_duplicate_guard_seconds(
    state: State<'_, AppState>,
    seconds: f64,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_duplicate_guard_seconds(seconds).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "連続入力抑制時間を保存できませんでした。",
        )
    })?;
    settings_snapshot(&state)
}

#[tauri::command]
pub fn set_detail_event_retention_days(
    state: State<'_, AppState>,
    days: u32,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_detail_event_retention_days(days).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "詳細ショートカット履歴の保持日数を保存できませんでした。",
        )
    })?;
    settings_snapshot(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_executable_name_is_reported_separately() {
        let invalid = add_excluded_app_error(&ExclusionCommandError::Storage(
            StorageError::InvalidExecutableName,
        ));
        assert_eq!(invalid.code, "INVALID_EXECUTABLE_NAME");
        assert_eq!(invalid.message, "除外アプリを追加できませんでした。");

        let other = add_excluded_app_error(&ExclusionCommandError::Storage(
            StorageError::InvalidDateRange,
        ));
        assert_eq!(other.code, "SETTINGS_FAILED");
        assert_eq!(other.message, "除外アプリを追加できませんでした。");
    }
}
