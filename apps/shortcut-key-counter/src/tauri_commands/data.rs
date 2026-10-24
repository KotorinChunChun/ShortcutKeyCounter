//! DB環境切替、サンプル生成、削除、CSV出力、バックアップ、保存先フォルダー。

use chrono::{Duration, Local};
use serde::Serialize;
use tauri::State;

use super::{db_lock, downloads_directory, parse_date, settings_db_lock};
use crate::app_state::AppState;
use crate::command_api;
use crate::command_error::CommandError;
use crate::database_environment::{DatabaseEnvironment, DatabaseEnvironmentSnapshot};
use crate::maintenance::{BackupResult, ClearResult, DeleteBeforePreview, ExportResult};
use crate::sample_data::{SampleGenerationResult, SampleSourceStatus};

#[derive(Clone, Debug, Serialize)]
pub struct AppPathsSnapshot {
    pub data_directory: String,
    pub database_path: String,
    pub logs_directory: String,
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
    // 前日イベントからview-modelだけを構成し、Capture外の別ログへ入力を永続化しない。
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
    let applications =
        command_api::list_daily_export_applications(&db).map_err(|_| CommandError::db())?;
    let (first_date, last_date) =
        command_api::daily_export_date_bounds(&db).map_err(|_| CommandError::db())?;
    drop(db);
    let settings = settings_db_lock(&state)?;
    let selected = command_api::daily_export_applications(&settings).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "日次CSVのアプリ選択を読み込めませんでした。",
        )
    })?;
    let name = command_api::daily_export_name(&settings).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "日次CSVの出力名を読み込めませんでした。")
    })?;
    Ok(DailyExportFilterSnapshot {
        applications,
        selected,
        first_date,
        last_date,
        name,
    })
}

#[tauri::command]
pub fn set_daily_export_filter(
    state: State<'_, AppState>,
    applications: Vec<String>,
    name: String,
) -> Result<Vec<String>, CommandError> {
    let settings = settings_db_lock(&state)?;
    command_api::set_daily_export_applications_command(&settings, &applications).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "日次CSVのアプリ選択を保存できませんでした。",
        )
    })?;
    command_api::set_daily_export_name_command(&settings, &name).map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "日次CSVの出力名を保存できませんでした。")
    })?;
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
    let directory = downloads_directory("EXPORT_FAILED")?;
    let start_date = start_date.as_deref().map(parse_date).transpose()?;
    let end_date = end_date.as_deref().map(parse_date).transpose()?;
    if let (Some(start), Some(end)) = (start_date, end_date) {
        if start > end {
            return Err(CommandError::new(
                "INVALID_DATE_RANGE",
                "開始日は終了日以前にしてください。",
            ));
        }
    }
    let db = db_lock(&state)?;
    command_api::export_daily_csv_command(
        &db,
        directory,
        applications.as_deref(),
        start_date,
        end_date,
        name.as_deref(),
    )
    .map_err(|_| CommandError::new("EXPORT_FAILED", "日次集計CSVを出力できませんでした。"))
}

#[tauri::command]
pub fn export_events_csv(state: State<'_, AppState>) -> Result<ExportResult, CommandError> {
    let directory = downloads_directory("EXPORT_FAILED")?;
    let db = db_lock(&state)?;
    command_api::export_events_csv_command(&db, directory).map_err(|_| {
        CommandError::new(
            "EXPORT_FAILED",
            "詳細ショートカット履歴CSVを出力できませんでした。",
        )
    })
}

#[tauri::command]
pub fn backup_database(state: State<'_, AppState>) -> Result<BackupResult, CommandError> {
    let directory = downloads_directory("BACKUP_FAILED")?;
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
