//! ダッシュボード、ランキング、推移グラフなどの統計参照。

use chrono::Local;
use shortcut_storage::statistics::{
    ApplicationStats, DailyPoint, DashboardSummary, DataBounds, HourlyTrend, RankedApplication,
    RankedShortcut, ShortcutDetail, ShortcutRankingRow,
};
use std::collections::BTreeMap;
use tauri::State;

use super::{db_lock, parse_date_range, settings_db_lock};
use crate::analytics::{self, AnalyticsRange, RankingsSnapshot, UsageGraphSnapshot};
use crate::app_state::AppState;
use crate::command_api;
use crate::command_error::CommandError;

#[tauri::command]
pub fn get_dashboard_summary(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<DashboardSummary, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::dashboard_summary(&db, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_shortcut_ranking(
    state: State<'_, AppState>,
    start: String,
    end: String,
    application: Option<String>,
    limit: u32,
) -> Result<Vec<RankedShortcut>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::shortcut_ranking(&db, start, end, application.as_deref(), limit)
        .map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_shortcut_ranking_extended(
    state: State<'_, AppState>,
    start: String,
    end: String,
    application: Option<String>,
    limit: u32,
) -> Result<Vec<ShortcutRankingRow>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::shortcut_ranking_extended(&db, start, end, application.as_deref(), limit)
        .map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_application_ranking(
    state: State<'_, AppState>,
    start: String,
    end: String,
    limit: u32,
) -> Result<Vec<RankedApplication>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::application_ranking(&db, start, end, limit).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_application_statistics(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<ApplicationStats>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::application_statistics(&db, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_daily_trend(
    state: State<'_, AppState>,
    start: String,
    end: String,
    shortcut: Option<String>,
    application: Option<String>,
) -> Result<Vec<DailyPoint>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::daily_trend(&db, start, end, shortcut.as_deref(), application.as_deref())
        .map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_hourly_trend(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<HourlyTrend, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::hourly_trend(&db, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_shortcut_detail(
    state: State<'_, AppState>,
    shortcut: String,
    start: String,
    end: String,
) -> Result<ShortcutDetail, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::shortcut_detail(&db, &shortcut, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn list_shortcuts(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<RankedShortcut>, CommandError> {
    let (start, end) = parse_date_range(&start, &end)?;
    let db = db_lock(&state)?;
    command_api::shortcut_catalog(&db, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_data_bounds(state: State<'_, AppState>) -> Result<DataBounds, CommandError> {
    let db = db_lock(&state)?;
    command_api::data_bounds(&db).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_rankings_v014(
    state: State<'_, AppState>,
    range: String,
    application: Option<String>,
) -> Result<RankingsSnapshot, CommandError> {
    let range = AnalyticsRange::parse(&range)
        .map_err(|_| CommandError::new("INVALID_RANGE", "ランキング期間が不正です。"))?;
    let db = db_lock(&state)?;
    analytics::rankings(
        &db,
        range,
        application.as_deref(),
        Local::now().naive_local(),
    )
    .map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_usage_graph_v014(
    state: State<'_, AppState>,
    mode: String,
    range: String,
    identities: Vec<String>,
) -> Result<UsageGraphSnapshot, CommandError> {
    let range = AnalyticsRange::parse(&range)
        .map_err(|_| CommandError::new("INVALID_RANGE", "グラフ期間が不正です。"))?;
    let db = db_lock(&state)?;
    analytics::usage_graph(&db, &mode, range, &identities, Local::now().naive_local())
        .map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_usage_graph_colors(
    state: State<'_, AppState>,
) -> Result<BTreeMap<String, String>, CommandError> {
    let db = settings_db_lock(&state)?;
    crate::settings::get_usage_graph_colors(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "グラフ色設定を読み込めませんでした。"))
}

#[tauri::command]
pub fn set_usage_graph_colors(
    state: State<'_, AppState>,
    colors: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, CommandError> {
    let db = settings_db_lock(&state)?;
    crate::settings::set_usage_graph_colors(&db, &colors).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "グラフ色設定を保存できませんでした。")
    })?;
    Ok(colors)
}
