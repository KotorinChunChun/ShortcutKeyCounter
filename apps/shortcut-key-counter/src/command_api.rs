//! UI/Tauriから接続する前段のバックエンドコマンド契約。
//!
//! Tauri依存はUI実装フェーズまで持ち込まず、引数・戻り値と処理順をRust APIとして固定する。

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use shortcut_storage::repositories::excluded_app_repository::{
    add_user_excluded_app, list_excluded_apps, remove_user_excluded_app, set_excluded_app_enabled,
    ExcludedApp,
};
use shortcut_storage::statistics::{
    get_application_ranking, get_application_statistics, get_daily_trend, get_dashboard_summary,
    get_data_bounds, get_shortcut_detail, get_shortcut_ranking, get_shortcut_ranking_extended,
    list_shortcuts, ApplicationStats, DailyPoint, DashboardSummary, DataBounds, DateRange,
    RankedApplication, RankedShortcut, ShortcutDetail, ShortcutRankingRow,
};
use shortcut_storage::StorageError;

use crate::capture_control::{CaptureControlError, CaptureController};
use crate::capture_worker::SharedExcludedApps;
use crate::exclusion_state::{reload_enabled_exclusions, ExclusionStateError};
use crate::key_policy::KeyAction;
use crate::maintenance::{
    backup_database, backup_database_to_directory, clear_all_usage_data, export_daily_csv_filtered,
    export_events_csv, BackupResult, ClearResult, ExportResult,
    MaintenanceError,
};
use crate::overlay::config::{
    OverlayConfig, OverlayDisplayMode, OverlayInAnimation, OverlayMonitorMode, OverlayOutAnimation,
    OverlayPosition, OverlayPositionMode, RepeatCounterTheme,
};
use crate::settings::{
    get_auto_start, get_capture_elevated, get_detail_event_retention_days,
    get_duplicate_guard_seconds, get_overlay_enabled, get_theme, get_usage_tracking_enabled,
    get_daily_export_applications, set_daily_export_applications,
    set_auto_start, set_capture_elevated, set_detail_event_retention_days,
    set_duplicate_guard_seconds, set_theme, set_usage_tracking_enabled, SettingsError,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureStatus {
    pub enabled: bool,
    pub dropped_queue_items: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsSnapshot {
    pub auto_start: bool,
    pub capture_elevated: bool,
    pub theme: String,
    pub overlay_enabled: bool,
    pub usage_tracking_enabled: bool,
    pub duplicate_guard_seconds: f64,
    pub detail_event_retention_days: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverlaySettingsDto {
    pub enabled: bool,
    pub display_duration_ms: u64,
    pub display_mode: OverlayDisplayMode,
    pub monitor_mode: OverlayMonitorMode,
    pub fixed_monitor_id: String,
    pub position_mode: OverlayPositionMode,
    pub fixed_position: OverlayPosition,
    pub allowed_positions: Vec<OverlayPosition>,
    pub preferred_position: OverlayPosition,
    pub max_rows: usize,
    pub background_color: String,
    pub background_opacity: f32,
    pub enter_animation: OverlayInAnimation,
    pub exit_animation: OverlayOutAnimation,
    pub enter_ms: u64,
    pub exit_ms: u64,
    pub repeat_counter: RepeatCounterTheme,
    pub default_single_key_action: KeyAction,
}

impl OverlaySettingsDto {
    pub fn from_parts(
        config: &OverlayConfig,
        enabled: bool,
        default_single_key_action: KeyAction,
    ) -> Self {
        Self {
            enabled,
            display_duration_ms: config.display_duration_ms,
            display_mode: config.display_mode,
            monitor_mode: config.monitor_mode,
            fixed_monitor_id: config.fixed_monitor_id.clone(),
            position_mode: config.position_mode,
            fixed_position: config.fixed_position,
            allowed_positions: config.allowed_positions.clone(),
            preferred_position: config.preferred_position,
            max_rows: config.max_rows,
            background_color: config.background.color.clone(),
            background_opacity: config.background.opacity,
            enter_animation: config.animation.enter,
            exit_animation: config.animation.exit,
            enter_ms: config.animation.enter_ms,
            exit_ms: config.animation.exit_ms,
            repeat_counter: config.repeat_counter.clone(),
            default_single_key_action,
        }
    }

    pub fn apply_to(&self, mut config: OverlayConfig) -> OverlayConfig {
        config.display_duration_ms = self.display_duration_ms;
        config.display_mode = self.display_mode;
        config.monitor_mode = self.monitor_mode;
        config.fixed_monitor_id = self.fixed_monitor_id.clone();
        config.position_mode = self.position_mode;
        config.fixed_position = self.fixed_position;
        config.allowed_positions = self.allowed_positions.clone();
        config.preferred_position = self.preferred_position;
        config.max_rows = self.max_rows;
        config.background.color = self.background_color.clone();
        config.background.opacity = self.background_opacity;
        config.animation.enter = self.enter_animation;
        config.animation.exit = self.exit_animation;
        config.animation.enter_ms = self.enter_ms;
        config.animation.exit_ms = self.exit_ms;
        config.repeat_counter = self.repeat_counter.clone();
        config.sanitize()
    }
}

pub fn get_capture_status(controller: &CaptureController) -> CaptureStatus {
    CaptureStatus {
        enabled: controller.is_enabled(),
        dropped_queue_items: controller.queue().dropped_count(),
    }
}

pub fn start_capture(controller: &mut CaptureController) -> Result<(), CaptureControlError> {
    controller.start_capture()
}

pub fn stop_capture(controller: &mut CaptureController) -> Result<(), CaptureControlError> {
    controller.stop_capture()
}

pub fn dashboard_summary(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<DashboardSummary, StorageError> {
    let range = DateRange::new(start, end)?;
    get_dashboard_summary(connection, &range)
}

pub fn shortcut_ranking(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    application: Option<&str>,
    limit: u32,
) -> Result<Vec<RankedShortcut>, StorageError> {
    let range = DateRange::new(start, end)?;
    get_shortcut_ranking(connection, &range, application, limit)
}

pub fn shortcut_ranking_extended(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    application: Option<&str>,
    limit: u32,
) -> Result<Vec<ShortcutRankingRow>, StorageError> {
    let range = DateRange::new(start, end)?;
    get_shortcut_ranking_extended(connection, &range, application, limit)
}

pub fn application_statistics(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<ApplicationStats>, StorageError> {
    let range = DateRange::new(start, end)?;
    get_application_statistics(connection, &range)
}

pub fn shortcut_catalog(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<RankedShortcut>, StorageError> {
    let range = DateRange::new(start, end)?;
    list_shortcuts(connection, &range)
}

pub fn data_bounds(connection: &Connection) -> Result<DataBounds, StorageError> {
    get_data_bounds(connection)
}

pub fn application_ranking(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    limit: u32,
) -> Result<Vec<RankedApplication>, StorageError> {
    let range = DateRange::new(start, end)?;
    get_application_ranking(connection, &range, limit)
}

pub fn daily_trend(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    shortcut: Option<&str>,
    application: Option<&str>,
) -> Result<Vec<DailyPoint>, StorageError> {
    let range = DateRange::new(start, end)?;
    get_daily_trend(connection, &range, shortcut, application)
}

pub fn shortcut_detail(
    connection: &Connection,
    shortcut: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<ShortcutDetail, StorageError> {
    let range = DateRange::new(start, end)?;
    get_shortcut_detail(connection, shortcut, &range)
}

pub fn get_excluded_apps(connection: &Connection) -> Result<Vec<ExcludedApp>, StorageError> {
    list_excluded_apps(connection)
}

pub fn add_excluded_app(
    connection: &Connection,
    executable_name: &str,
) -> Result<(), StorageError> {
    add_user_excluded_app(connection, executable_name)
}

pub fn set_excluded_app_enabled_command(
    connection: &Connection,
    executable_name: &str,
    enabled: bool,
) -> Result<bool, StorageError> {
    set_excluded_app_enabled(connection, executable_name, enabled)
}

pub fn remove_user_excluded_app_command(
    connection: &Connection,
    executable_name: &str,
) -> Result<bool, StorageError> {
    remove_user_excluded_app(connection, executable_name)
}

pub fn add_excluded_app_and_reload(
    connection: &Connection,
    shared: &SharedExcludedApps,
    executable_name: &str,
) -> Result<(), ExclusionCommandError> {
    add_user_excluded_app(connection, executable_name)?;
    reload_enabled_exclusions(connection, shared)?;
    Ok(())
}

pub fn set_excluded_app_enabled_and_reload(
    connection: &Connection,
    shared: &SharedExcludedApps,
    executable_name: &str,
    enabled: bool,
) -> Result<bool, ExclusionCommandError> {
    let changed = set_excluded_app_enabled(connection, executable_name, enabled)?;
    if changed {
        reload_enabled_exclusions(connection, shared)?;
    }
    Ok(changed)
}

pub fn remove_user_excluded_app_and_reload(
    connection: &Connection,
    shared: &SharedExcludedApps,
    executable_name: &str,
) -> Result<bool, ExclusionCommandError> {
    let changed = remove_user_excluded_app(connection, executable_name)?;
    if changed {
        reload_enabled_exclusions(connection, shared)?;
    }
    Ok(changed)
}

#[derive(Debug, thiserror::Error)]
pub enum ExclusionCommandError {
    #[error("除外アプリDB操作に失敗しました: {0}")]
    Storage(#[from] StorageError),
    #[error("除外アプリ状態の反映に失敗しました: {0}")]
    Reload(#[from] ExclusionStateError),
}

pub fn get_settings(connection: &Connection) -> Result<SettingsSnapshot, SettingsError> {
    Ok(SettingsSnapshot {
        auto_start: get_auto_start(connection)?,
        capture_elevated: get_capture_elevated(connection)?,
        theme: get_theme(connection)?,
        overlay_enabled: get_overlay_enabled(connection)?,
        usage_tracking_enabled: get_usage_tracking_enabled(connection)?,
        duplicate_guard_seconds: get_duplicate_guard_seconds(connection)?,
        detail_event_retention_days: get_detail_event_retention_days(connection)?,
    })
}

pub fn set_auto_start_command(
    connection: &Connection,
    executable_path: impl AsRef<std::path::Path>,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_auto_start(connection, executable_path, enabled)
}

pub fn set_capture_elevated_command(
    connection: &Connection,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_capture_elevated(connection, enabled)
}

pub fn set_theme_command(connection: &Connection, theme: &str) -> Result<(), SettingsError> {
    set_theme(connection, theme)
}

pub fn set_usage_tracking_command(
    connection: &Connection,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_usage_tracking_enabled(connection, enabled)
}

pub fn set_duplicate_guard_seconds_command(
    connection: &Connection,
    seconds: f64,
) -> Result<(), SettingsError> {
    set_duplicate_guard_seconds(connection, seconds)
}

pub fn set_detail_event_retention_days_command(
    connection: &Connection,
    days: u32,
) -> Result<(), SettingsError> {
    set_detail_event_retention_days(connection, days)
}

pub fn export_daily_csv_command(
    connection: &Connection,
    output_directory: impl AsRef<std::path::Path>,
    applications: Option<&[String]>,
) -> Result<ExportResult, MaintenanceError> {
    export_daily_csv_filtered(connection, output_directory, applications)
}

pub fn list_daily_export_applications(
    connection: &Connection,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT application FROM shortcut_daily WHERE application <> '' ORDER BY application COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub fn daily_export_applications(
    connection: &Connection,
) -> Result<Option<Vec<String>>, SettingsError> {
    get_daily_export_applications(connection)
}

pub fn set_daily_export_applications_command(
    connection: &Connection,
    applications: &[String],
) -> Result<(), SettingsError> {
    set_daily_export_applications(connection, applications)
}

pub fn export_events_csv_command(
    connection: &Connection,
    output_directory: impl AsRef<std::path::Path>,
) -> Result<ExportResult, MaintenanceError> {
    export_events_csv(connection, output_directory)
}

pub fn backup_database_command(
    connection: &Connection,
    output_path: impl AsRef<std::path::Path>,
) -> Result<BackupResult, MaintenanceError> {
    backup_database(connection, output_path)
}

pub fn backup_database_to_directory_command(
    connection: &Connection,
    output_directory: impl AsRef<std::path::Path>,
) -> Result<BackupResult, MaintenanceError> {
    backup_database_to_directory(connection, output_directory)
}

pub fn clear_all_usage_data_command(
    connection: &mut Connection,
) -> Result<ClearResult, MaintenanceError> {
    clear_all_usage_data(connection)
}
