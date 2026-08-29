//! UIやHook開始より前に行うアプリケーション初期化。

use std::collections::HashSet;

use chrono::Local;
use rusqlite::Connection;
use shortcut_common::paths::AppPaths;
use shortcut_storage::db::open_database;
use shortcut_storage::repositories::excluded_app_repository::{
    load_enabled_excluded_apps, seed_default_excluded_apps,
};
use shortcut_storage::repositories::settings_repository::seed_default_settings;
use shortcut_storage::retention::cleanup_expired_events_with_days;
use shortcut_storage::{migration::run_migrations, StorageError};
use shortcut_windows::known_folders::{
    ensure_app_directories, local_app_data_paths, KnownFoldersError,
};
use thiserror::Error;

use crate::diagnostics::{DiagnosticEvent, DiagnosticLogError, DiagnosticLogger};

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("アプリケーションパスを解決できません: {0}")]
    Paths(#[from] KnownFoldersError),
    #[error("データベース初期化に失敗しました: {0}")]
    Storage(#[from] StorageError),
    #[error("診断ログを初期化できません: {0}")]
    Diagnostics(#[from] DiagnosticLogError),
}

/// Hook開始前に確立された起動コンテキスト。
pub struct StartupContext {
    pub paths: AppPaths,
    pub connection: Connection,
    pub excluded_apps: HashSet<String>,
    pub logger: DiagnosticLogger,
}

/// LocalAppDataを解決し、DB・migration・seed・retentionを完了して返す。
pub fn initialize() -> Result<StartupContext, StartupError> {
    let paths = local_app_data_paths()?;
    initialize_at(paths)
}

/// テストやホスト側のパス注入用の初期化関数。
pub fn initialize_at(paths: AppPaths) -> Result<StartupContext, StartupError> {
    ensure_app_directories(&paths)?;
    let logger = DiagnosticLogger::new(&paths.logs_dir)?;
    logger.record(DiagnosticEvent::AppStarted {
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })?;

    let mut connection = match open_database(&paths.database_path) {
        Ok(connection) => connection,
        Err(error) => {
            let _ = logger.record(DiagnosticEvent::DatabaseOpenFailed);
            return Err(error.into());
        }
    };
    logger.record(DiagnosticEvent::DatabaseOpened)?;

    if let Err(error) = run_migrations(&mut connection) {
        let _ = logger.record(DiagnosticEvent::MigrationFailed);
        return Err(error.into());
    }
    logger.record(DiagnosticEvent::MigrationCompleted {
        schema_version: shortcut_storage::migration::CURRENT_SCHEMA_VERSION,
    })?;
    seed_default_settings(&connection)?;
    seed_default_excluded_apps(&connection)?;

    let retention_days = crate::settings::get_detail_event_retention_days(&connection).unwrap_or(7);
    let deleted_events = cleanup_expired_events_with_days(
        &connection,
        Local::now().naive_local(),
        i64::from(retention_days),
    )?;
    logger.record(DiagnosticEvent::CleanupCompleted {
        deleted_events: deleted_events as u64,
    })?;
    let excluded_apps = load_enabled_excluded_apps(&connection)?
        .into_iter()
        .collect::<HashSet<_>>();

    Ok(StartupContext {
        paths,
        connection,
        excluded_apps,
        logger,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn paths() -> AppPaths {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        AppPaths::from_local_app_data(
            std::env::temp_dir().join(format!("shortcut-key-counter-startup-{unique}")),
        )
    }

    #[test]
    fn initialization_completes_before_hook_start_and_seeds_defaults() {
        let paths = paths();
        let context = initialize_at(paths.clone()).expect("startup");
        assert!(context.paths.database_path.exists());
        assert_eq!(context.excluded_apps.len(), 11);
        let settings: i64 = context
            .connection
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .expect("settings");
        assert_eq!(settings, 8);
        let _ = fs::remove_dir_all(paths.app_dir.parent().expect("temp root"));
    }

    #[test]
    fn migration_failure_does_not_delete_existing_database_file() {
        let paths = paths();
        fs::create_dir_all(&paths.app_dir).expect("app directory");
        fs::write(&paths.database_path, b"not a sqlite database").expect("invalid database");
        let result = initialize_at(paths.clone());
        assert!(result.is_err());
        assert!(paths.database_path.exists());
        let _ = fs::remove_dir_all(paths.app_dir.parent().expect("temp root"));
    }
}
