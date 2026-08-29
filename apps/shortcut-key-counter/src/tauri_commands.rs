//! WebViewからRust backendへ接続するTauri command群。

use chrono::{Duration, Local, NaiveDate};
use serde::Serialize;
use shortcut_core::key_mapping::KeyMapping;
use shortcut_storage::repositories::excluded_app_repository::ExcludedApp;
use shortcut_storage::statistics::{
    ApplicationStats, DailyPoint, DashboardSummary, DataBounds, RankedApplication, RankedShortcut,
    HourlyTrend, ShortcutDetail, ShortcutRankingRow,
};
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager, State};

use crate::analytics::{self, AnalyticsRange, RankingsSnapshot, UsageGraphSnapshot};
use crate::app_info::{self, AppInfo};
use crate::app_state::AppState;
use crate::capture_health::CaptureHealthSnapshot;
use crate::command_api::{self, CaptureStatus, OverlaySettingsDto, SettingsSnapshot};
use crate::command_error::CommandError;
use crate::database_environment::{DatabaseEnvironment, DatabaseEnvironmentSnapshot};
use crate::key_mapping_state::{self, KeyMappingExportResult};
use crate::key_policy::KeyPolicyConfig;
use crate::maintenance::{
    downloads_or_fallback, BackupResult, ClearResult, DeleteBeforePreview, ExportResult,
};
use crate::overlay::config::{KeyboardLayout, OverlayKeyLabels};
use crate::realtime_log::RealtimeLogEntry;
use crate::sample_data::{SampleGenerationResult, SampleSourceStatus};
use crate::tray::{sync_capture_ui, sync_overlay_ui};

#[derive(Clone, Debug, Serialize)]
pub struct OverlayMonitorDto {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppPathsSnapshot {
    pub data_directory: String,
    pub database_path: String,
    pub logs_directory: String,
}

fn parse_date(value: &str) -> Result<NaiveDate, CommandError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CommandError::new("INVALID_DATE", "日付の形式が不正です。"))
}

fn db_lock(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, CommandError> {
    state.database.lock().map_err(|_| CommandError::state())
}

fn settings_db_lock(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, CommandError> {
    state
        .settings_database
        .lock()
        .map_err(|_| CommandError::state())
}

fn capture_status(state: &AppState) -> Result<CaptureStatus, CommandError> {
    Ok(CaptureStatus {
        enabled: state.capture_enabled().map_err(|_| CommandError::state())?,
        dropped_queue_items: state
            .dropped_queue_items()
            .map_err(|_| CommandError::state())?,
    })
}

#[tauri::command]
pub fn get_capture_status(state: State<'_, AppState>) -> Result<CaptureStatus, CommandError> {
    capture_status(&state)
}

#[tauri::command]
pub fn set_capture_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<CaptureStatus, CommandError> {
    state.set_capture_enabled(enabled).map_err(|_| {
        CommandError::new(
            if enabled {
                "CAPTURE_START_FAILED"
            } else {
                "CAPTURE_STOP_FAILED"
            },
            if enabled {
                "計測を開始できませんでした。"
            } else {
                "計測を停止できませんでした。"
            },
        )
    })?;
    sync_capture_ui(&app, enabled);
    capture_status(&state)
}

#[tauri::command]
pub fn get_capture_health(state: State<'_, AppState>) -> CaptureHealthSnapshot {
    state.capture_health()
}

#[tauri::command]
pub fn get_dashboard_summary(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<DashboardSummary, CommandError> {
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
    let db = db_lock(&state)?;
    command_api::application_ranking(&db, start, end, limit).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_application_statistics(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<ApplicationStats>, CommandError> {
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
    let db = db_lock(&state)?;
    command_api::shortcut_detail(&db, &shortcut, start, end).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn list_shortcuts(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<RankedShortcut>, CommandError> {
    let start = parse_date(&start)?;
    let end = parse_date(&end)?;
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
        .map_err(|error| {
            let code = if error.to_string().contains("実行ファイル名が不正") {
                "INVALID_EXECUTABLE_NAME"
            } else {
                "SETTINGS_FAILED"
            };
            CommandError::new(code, "除外アプリを追加できませんでした。")
        })?;
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

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsSnapshot, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
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
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
}

#[tauri::command]
pub fn set_theme(
    state: State<'_, AppState>,
    theme: String,
) -> Result<SettingsSnapshot, CommandError> {
    let db = settings_db_lock(&state)?;
    command_api::set_theme_command(&db, &theme)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "テーマ設定を保存できませんでした。"))?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
}

#[tauri::command]
pub fn set_overlay_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_overlay_enabled(enabled).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "オーバーレイ設定を保存できませんでした。",
        )
    })?;
    sync_overlay_ui(&app, enabled);
    let db = settings_db_lock(&state)?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
}

fn overlay_settings_snapshot(state: &AppState) -> Result<OverlaySettingsDto, CommandError> {
    let enabled = state.overlay_enabled().map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "オーバーレイ設定を読み込めませんでした。",
        )
    })?;
    let policy = state.key_policy_snapshot().map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "キーポリシーを読み込めませんでした。")
    })?;
    Ok(OverlaySettingsDto::from_parts(
        &state.overlay_config(),
        enabled,
        policy.default_single_key,
    ))
}

#[tauri::command]
pub fn get_overlay_settings(
    state: State<'_, AppState>,
) -> Result<OverlaySettingsDto, CommandError> {
    overlay_settings_snapshot(&state)
}

#[tauri::command]
pub fn get_overlay_monitors() -> Vec<OverlayMonitorDto> {
    shortcut_windows::overlay_window::available_monitors()
        .into_iter()
        .map(|monitor| OverlayMonitorDto {
            label: monitor.label,
            id: monitor.id,
        })
        .collect()
}

#[tauri::command]
pub fn preview_overlay_monitor(state: State<'_, AppState>, monitor_id: String, number: u32) {
    state
        .overlay
        .preview_monitor(monitor_id, format!("モニター {number}"));
}

#[tauri::command]
pub fn set_overlay_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: OverlaySettingsDto,
) -> Result<OverlaySettingsDto, CommandError> {
    let config = settings.apply_to(state.overlay_config());
    state
        .apply_overlay_settings(config, settings.enabled, settings.default_single_key_action)
        .map_err(|_| {
            CommandError::new(
                "SETTINGS_FAILED",
                "オーバーレイ設定を保存できませんでした。",
            )
        })?;
    sync_overlay_ui(&app, settings.enabled);
    overlay_settings_snapshot(&state)
}

#[tauri::command]
pub fn get_overlay_key_labels(state: State<'_, AppState>) -> OverlayKeyLabels {
    state.overlay_labels()
}

#[tauri::command]
pub fn set_overlay_key_labels(
    state: State<'_, AppState>,
    labels: OverlayKeyLabels,
) -> Result<OverlayKeyLabels, CommandError> {
    state
        .replace_overlay_labels(labels)
        .map_err(|_| CommandError::new("KEY_LABELS_FAILED", "表示名を保存できませんでした。"))
}

#[derive(Clone, Debug, Serialize)]
pub struct DailyExportFilterSnapshot {
    pub applications: Vec<String>,
    pub selected: Option<Vec<String>>,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub name: String,
}

#[tauri::command]
pub fn set_overlay_keyboard_layout(
    state: State<'_, AppState>,
    layout: KeyboardLayout,
) -> Result<OverlayKeyLabels, CommandError> {
    state.replace_overlay_keyboard_layout(layout).map_err(|_| {
        CommandError::new(
            "KEY_LABELS_FAILED",
            "キーボード種類を保存できませんでした。",
        )
    })
}

#[tauri::command]
pub fn get_overlay_key_policy(state: State<'_, AppState>) -> Result<KeyPolicyConfig, CommandError> {
    state
        .key_policy_snapshot()
        .map_err(|_| CommandError::new("KEY_POLICY_FAILED", "キーポリシーを読み込めませんでした。"))
}

#[tauri::command]
pub fn set_overlay_key_policy(
    state: State<'_, AppState>,
    policy: KeyPolicyConfig,
) -> Result<KeyPolicyConfig, CommandError> {
    state
        .replace_key_policy(policy)
        .map_err(|_| CommandError::new("KEY_POLICY_FAILED", "キーポリシーを保存できませんでした。"))
}

#[tauri::command]
pub fn reload_overlay_assets(state: State<'_, AppState>) {
    state.reload_overlay_assets();
}

#[tauri::command]
pub async fn set_capture_elevated(
    app: AppHandle,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    // UAC / Named Pipe handshake / cleanupはblocking Windows APIを含むため、
    // Tauriのblocking poolへ退避してWebView event loopを止めない。
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.set_elevated_enabled(enabled).map_err(|error| {
            let text = error.to_string();
            if text.contains("キャンセル") {
                CommandError::new(
                    "UAC_CANCELLED",
                    "管理者権限モードは有効化されませんでした。",
                )
            } else {
                CommandError::new(
                    "SETTINGS_FAILED",
                    format!("管理者権限モードを切り替えられませんでした: {text}"),
                )
            }
        })?;
        let db = settings_db_lock(&state)?;
        command_api::get_settings(&db)
            .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
    })
    .await
    .map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "管理者権限モードの処理スレッドが異常終了しました。",
        )
    })?
}

#[tauri::command]
pub fn set_usage_tracking(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_usage_tracking(enabled).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "利用時間設定を保存できませんでした。")
    })?;
    let db = settings_db_lock(&state)?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
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
    let db = settings_db_lock(&state)?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
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
    let db = settings_db_lock(&state)?;
    command_api::get_settings(&db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
}

#[tauri::command]
pub fn preview_overlay_sample(state: State<'_, AppState>) {
    state.overlay.preview_sample();
}

#[tauri::command]
pub fn begin_key_search(state: State<'_, AppState>) {
    state.key_search.begin();
}

#[tauri::command]
pub fn poll_key_search(state: State<'_, AppState>) -> Option<String> {
    state.key_search.take_result()
}

#[tauri::command]
pub fn cancel_key_search(state: State<'_, AppState>) {
    state.key_search.cancel();
}

#[tauri::command]
pub fn get_database_environment(state: State<'_, AppState>) -> DatabaseEnvironmentSnapshot {
    state.database_environment()
}

#[tauri::command]
pub fn switch_database_environment(
    state: State<'_, AppState>,
    environment: DatabaseEnvironment,
) -> Result<DatabaseEnvironmentSnapshot, CommandError> {
    state.switch_database(environment).map_err(|error| {
        CommandError::new(
            "DB_SWITCH_FAILED",
            format!("データベースを切り替えられませんでした: {error}"),
        )
    })
}

#[tauri::command]
pub fn get_sample_source_status(
    state: State<'_, AppState>,
) -> Result<SampleSourceStatus, CommandError> {
    let production = settings_db_lock(&state)?;
    crate::sample_data::source_status(&production)
        .map_err(|_| CommandError::new("SAMPLE_FAILED", "サンプル生成元を確認できませんでした。"))
}

#[tauri::command]
pub fn generate_sample_data(
    state: State<'_, AppState>,
) -> Result<SampleGenerationResult, CommandError> {
    if state.active_database.current() != DatabaseEnvironment::Test {
        return Err(CommandError::new(
            "SAMPLE_PRODUCTION_FORBIDDEN",
            "サンプルデータは検証DBでのみ生成できます。",
        ));
    }
    let production = settings_db_lock(&state)?;
    let mut target = db_lock(&state)?;
    let mut result = crate::sample_data::generate_sample_data(&production, &mut target, 10_000)
        .map_err(|error| {
            CommandError::new(
                "SAMPLE_FAILED",
                format!("サンプルデータを生成できませんでした: {error}"),
            )
        })?;

    // Realtime Log用に偽Captureを注入せず、検証DBへ生成済みの「DB記録対象」
    // 前日イベントからview-modelだけを構成する。修飾なし単独入力を別ログへ永続化しない。
    let yesterday = Local::now().date_naive() - Duration::days(1);
    let realtime_samples = crate::sample_data::previous_day_realtime_samples(
        &target, yesterday, 200,
    )
    .map_err(|error| {
        CommandError::new(
            "SAMPLE_FAILED",
            format!("前日Realtime Logサンプルを読み込めませんでした: {error}"),
        )
    })?;
    drop(target);
    drop(production);

    state.realtime_log.clear();
    for (occurred_at, shortcut, application) in &realtime_samples {
        state.realtime_log.push(
            *occurred_at,
            shortcut.clone(),
            application.clone(),
            "sample",
        );
    }
    result.realtime_log_rows = realtime_samples.len() as u64;
    Ok(result)
}

#[tauri::command]
pub fn preview_delete_yesterday_and_earlier(
    state: State<'_, AppState>,
) -> Result<DeleteBeforePreview, CommandError> {
    let cutoff = Local::now().date_naive() - Duration::days(1);
    let db = db_lock(&state)?;
    crate::maintenance::preview_delete_on_or_before(&db, cutoff).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn delete_yesterday_and_earlier(
    state: State<'_, AppState>,
) -> Result<DeleteBeforePreview, CommandError> {
    let cutoff = Local::now().date_naive() - Duration::days(1);
    let mut db = db_lock(&state)?;
    crate::maintenance::delete_on_or_before(&mut db, cutoff).map_err(|_| CommandError::db())
}

#[tauri::command]
pub fn get_daily_export_filter(
    state: State<'_, AppState>,
) -> Result<DailyExportFilterSnapshot, CommandError> {
    let db = db_lock(&state)?;
    let applications = command_api::list_daily_export_applications(&db).map_err(|_| CommandError::db())?;
    let (first_date, last_date) = command_api::daily_export_date_bounds(&db).map_err(|_| CommandError::db())?;
    drop(db);
    let settings = settings_db_lock(&state)?;
    let selected = command_api::daily_export_applications(&settings)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "日次CSVのアプリ選択を読み込めませんでした。"))?;
    let name = command_api::daily_export_name(&settings)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "日次CSVの出力名を読み込めませんでした。"))?;
    Ok(DailyExportFilterSnapshot { applications, selected, first_date, last_date, name })
}

#[tauri::command]
pub fn set_daily_export_filter(
    state: State<'_, AppState>,
    applications: Vec<String>,
    name: String,
) -> Result<Vec<String>, CommandError> {
    let settings = settings_db_lock(&state)?;
    command_api::set_daily_export_applications_command(&settings, &applications)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "日次CSVのアプリ選択を保存できませんでした。"))?;
    command_api::set_daily_export_name_command(&settings, &name)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "日次CSVの出力名を保存できませんでした。"))?;
    Ok(applications)
}

#[tauri::command]
pub fn export_daily_csv(
    state: State<'_, AppState>,
    applications: Option<Vec<String>>,
    start_date: Option<String>,
    end_date: Option<String>,
    name: Option<String>,
) -> Result<ExportResult, CommandError> {
    let directory = downloads_or_fallback(None).map_err(|_| {
        CommandError::new(
            "EXPORT_FAILED",
            "Downloadsフォルダーを取得できませんでした。",
        )
    })?;
    let start_date = start_date.as_deref().map(parse_date).transpose()?;
    let end_date = end_date.as_deref().map(parse_date).transpose()?;
    if let (Some(start), Some(end)) = (start_date, end_date) {
        if start > end {
            return Err(CommandError::new("INVALID_DATE_RANGE", "開始日は終了日以前にしてください。"));
        }
    }
    let db = db_lock(&state)?;
    command_api::export_daily_csv_command(&db, directory, applications.as_deref(), start_date, end_date, name.as_deref())
        .map_err(|_| CommandError::new("EXPORT_FAILED", "日次集計CSVを出力できませんでした。"))
}

#[tauri::command]
pub fn export_events_csv(state: State<'_, AppState>) -> Result<ExportResult, CommandError> {
    let directory = downloads_or_fallback(None).map_err(|_| {
        CommandError::new(
            "EXPORT_FAILED",
            "Downloadsフォルダーを取得できませんでした。",
        )
    })?;
    let db = db_lock(&state)?;
    command_api::export_events_csv_command(&db, directory)
        .map_err(|_| CommandError::new("EXPORT_FAILED", "詳細ショートカット履歴CSVを出力できませんでした。"))
}

#[tauri::command]
pub fn backup_database(state: State<'_, AppState>) -> Result<BackupResult, CommandError> {
    let directory = downloads_or_fallback(None).map_err(|_| {
        CommandError::new(
            "BACKUP_FAILED",
            "Downloadsフォルダーを取得できませんでした。",
        )
    })?;
    let db = db_lock(&state)?;
    command_api::backup_database_to_directory_command(&db, directory)
        .map_err(|_| CommandError::new("BACKUP_FAILED", "DBバックアップを作成できませんでした。"))
}

#[tauri::command]
pub fn delete_all_statistics(state: State<'_, AppState>) -> Result<ClearResult, CommandError> {
    let mut database = db_lock(&state)?;
    command_api::clear_all_usage_data_command(&mut database)
        .map_err(|_| CommandError::new("DELETE_FAILED", "統計データを削除できませんでした。"))
}

#[tauri::command]
pub fn get_app_paths(state: State<'_, AppState>) -> AppPathsSnapshot {
    AppPathsSnapshot {
        data_directory: state.paths.app_dir.to_string_lossy().into_owned(),
        database_path: state.paths.database_path.to_string_lossy().into_owned(),
        logs_directory: state.paths.logs_dir.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(&state.paths.app_dir).map_err(|_| {
        CommandError::new(
            "OPEN_FOLDER_FAILED",
            "データ保存フォルダーを開けませんでした。",
        )
    })
}

#[tauri::command]
pub fn open_overlay_labels(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.labels_path())
        .map_err(|_| CommandError::new("OPEN_FILE_FAILED", "表示名JSONを開けませんでした。"))
}

#[tauri::command]
pub fn open_overlay_theme(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.theme_path()).map_err(|_| {
        CommandError::new(
            "OPEN_FILE_FAILED",
            "オーバーレイ設定JSONを開けませんでした。",
        )
    })
}

#[tauri::command]
pub fn open_overlay_policy(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.policy_path())
        .map_err(|_| CommandError::new("OPEN_FILE_FAILED", "キーポリシーJSONを開けませんでした。"))
}

#[tauri::command]
pub fn open_overlay_design_folder(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.design_directory()).map_err(|_| {
        CommandError::new(
            "OPEN_FOLDER_FAILED",
            "オーバーレイデザインフォルダーを開けませんでした。",
        )
    })
}

#[tauri::command]
pub fn open_parent_folder(path: String) -> Result<(), CommandError> {
    let target = std::path::PathBuf::from(path);
    let directory = if target.is_dir() {
        target
    } else {
        target
            .parent()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| {
                CommandError::new(
                    "OPEN_FOLDER_FAILED",
                    "保存先フォルダーを特定できませんでした。",
                )
            })?
    };
    shortcut_windows::shell_open::open_path(&directory).map_err(|_| {
        CommandError::new("OPEN_FOLDER_FAILED", "保存先フォルダーを開けませんでした。")
    })
}

#[tauri::command]
pub fn get_key_mapping(state: State<'_, AppState>) -> Result<KeyMapping, CommandError> {
    key_mapping_state::snapshot(&state.key_mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングを読み込めませんでした。",
        )
    })
}

#[tauri::command]
pub fn set_key_mapping(
    state: State<'_, AppState>,
    mapping: KeyMapping,
) -> Result<KeyMapping, CommandError> {
    let path = key_mapping_state::mapping_path(&state.paths.app_dir);
    key_mapping_state::replace_and_save(&path, &state.key_mapping, mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングを保存できませんでした。",
        )
    })
}

#[tauri::command]
pub fn export_key_mapping_json(
    state: State<'_, AppState>,
) -> Result<KeyMappingExportResult, CommandError> {
    let directory = downloads_or_fallback(None).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "Downloadsフォルダーを取得できませんでした。",
        )
    })?;
    key_mapping_state::export_json(&directory, &state.key_mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングJSONを出力できませんでした。",
        )
    })
}

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
