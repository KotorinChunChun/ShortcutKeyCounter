//! CSV出力、SQLiteバックアップ、使用データ全削除を提供する。

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate};
use rusqlite::{backup::Backup, Connection, TransactionBehavior};
use serde::Serialize;
use shortcut_storage::StorageError;
use shortcut_windows::known_folders::{downloads_directory, KnownFoldersError};
use thiserror::Error;

const DAILY_FILE_PREFIX: &str = "ShortcutKeyCounter_daily_";
const EVENTS_FILE_PREFIX: &str = "ShortcutKeyCounter_events_";
const BACKUP_FILE_PREFIX: &str = "ShortcutKeyCounter_DB_";

#[derive(Debug, Error)]
pub enum MaintenanceError {
    #[error("Storage処理に失敗しました: {0}")]
    Storage(#[from] StorageError),
    #[error("SQLite処理に失敗しました: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("ファイル処理に失敗しました: {0}")]
    Io(#[from] io::Error),
    #[error("Downloadsフォルダーを解決できず、保存先も指定されていません: {0}")]
    DownloadsUnavailable(KnownFoldersError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExportResult {
    pub path: PathBuf,
    pub rows: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BackupResult {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClearResult {
    pub events_deleted: u64,
    pub daily_deleted: u64,
    pub usage_deleted: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeleteBeforePreview {
    pub cutoff_date: String,
    pub events: u64,
    pub daily_rows: u64,
    pub usage_rows: u64,
}

/// Downloadsを優先し、Known Folder解決失敗時だけUIから渡された保存先へ切り替える。
pub fn downloads_or_fallback(
    fallback_directory: Option<PathBuf>,
) -> Result<PathBuf, MaintenanceError> {
    resolve_export_directory(downloads_directory(), fallback_directory)
}

/// Downloads解決結果とUI側の保存先を分離してテストできるようにする。
pub fn resolve_export_directory(
    downloads: Result<PathBuf, KnownFoldersError>,
    fallback_directory: Option<PathBuf>,
) -> Result<PathBuf, MaintenanceError> {
    match downloads {
        Ok(path) => Ok(path),
        Err(error) => fallback_directory.ok_or(MaintenanceError::DownloadsUnavailable(error)),
    }
}

/// shortcut_dailyをUTF-8 BOM付きCSVへストリーミング出力する。
pub fn export_daily_csv(
    connection: &Connection,
    output_directory: impl AsRef<Path>,
) -> Result<ExportResult, MaintenanceError> {
    export_daily_csv_filtered(connection, output_directory, None)
}

/// 指定したアプリだけをshortcut_dailyからCSVへ出力する。Noneは全アプリを表す。
pub fn export_daily_csv_filtered(
    connection: &Connection,
    output_directory: impl AsRef<Path>,
    applications: Option<&[String]>,
) -> Result<ExportResult, MaintenanceError> {
    export_daily_csv_at(connection, output_directory.as_ref(), Local::now(), applications)
}

fn export_daily_csv_at(
    connection: &Connection,
    output_directory: &Path,
    timestamp: DateTime<Local>,
    applications: Option<&[String]>,
) -> Result<ExportResult, MaintenanceError> {
    let path = output_directory.join(format!(
        "{DAILY_FILE_PREFIX}{}.csv",
        timestamp.format("%Y%m%d_%H%M%S")
    ));
    let rows = write_export_file(&path, |writer| {
        write_csv_record(writer, ["date", "shortcut", "application", "count"])?;
        let mut row_count = 0_u64;
        match applications {
            Some(applications) if applications.is_empty() => {}
            Some(applications) => {
                let placeholders = std::iter::repeat("?")
                    .take(applications.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT date, shortcut, application, count FROM shortcut_daily WHERE application IN ({placeholders}) ORDER BY date ASC, shortcut ASC, application ASC"
                );
                let mut statement = connection.prepare(&sql)?;
                let mut result_rows = statement.query(rusqlite::params_from_iter(applications))?;
                while let Some(row) = result_rows.next()? {
                    write_daily_row(writer, row)?;
                    row_count += 1;
                }
            }
            None => {
                let mut statement = connection.prepare(
                    "SELECT date, shortcut, application, count FROM shortcut_daily ORDER BY date ASC, shortcut ASC, application ASC",
                )?;
                let mut result_rows = statement.query([])?;
                while let Some(row) = result_rows.next()? {
                    write_daily_row(writer, row)?;
                    row_count += 1;
                }
            }
        }
        Ok(row_count)
    })?;
    Ok(ExportResult { path, rows })
}

fn write_daily_row(
    writer: &mut BufWriter<File>,
    row: &rusqlite::Row<'_>,
) -> Result<(), MaintenanceError> {
    let date: String = row.get(0)?;
    let shortcut: String = row.get(1)?;
    let application: String = row.get(2)?;
    let count: i64 = row.get(3)?;
    write_csv_record(
        writer,
        [date.as_str(), shortcut.as_str(), application.as_str(), &count.to_string()],
    )?;
    Ok(())
}

/// shortcut_eventsをUTF-8 BOM付きCSVへストリーミング出力する。
pub fn export_events_csv(
    connection: &Connection,
    output_directory: impl AsRef<Path>,
) -> Result<ExportResult, MaintenanceError> {
    export_events_csv_at(connection, output_directory.as_ref(), Local::now())
}

fn export_events_csv_at(
    connection: &Connection,
    output_directory: &Path,
    timestamp: DateTime<Local>,
) -> Result<ExportResult, MaintenanceError> {
    let path = output_directory.join(format!(
        "{EVENTS_FILE_PREFIX}{}.csv",
        timestamp.format("%Y%m%d_%H%M%S")
    ));
    let rows = write_export_file(&path, |writer| {
        write_csv_record(writer, ["timestamp", "shortcut", "application"])?;
        let mut statement = connection.prepare(
            "SELECT occurred_at, shortcut, application
             FROM shortcut_events
             ORDER BY occurred_at ASC, id ASC",
        )?;
        let mut result_rows = statement.query([])?;
        let mut row_count = 0_u64;
        while let Some(row) = result_rows.next()? {
            let occurred_at: String = row.get(0)?;
            let shortcut: String = row.get(1)?;
            let application: String = row.get(2)?;
            write_csv_record(
                writer,
                [
                    occurred_at.as_str(),
                    shortcut.as_str(),
                    application.as_str(),
                ],
            )?;
            row_count += 1;
        }
        Ok(row_count)
    })?;
    Ok(ExportResult { path, rows })
}

/// SQLite Online Backup APIで稼働中DBを別ファイルへバックアップする。
pub fn backup_database(
    connection: &Connection,
    output_path: impl AsRef<Path>,
) -> Result<BackupResult, MaintenanceError> {
    let output_path = output_path.as_ref().to_path_buf();
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let result = backup_database_inner(connection, &output_path);
    if result.is_err() {
        let _ = fs::remove_file(&output_path);
    }
    result.map(|()| BackupResult { path: output_path })
}

/// Downloadsなど指定ディレクトリへ規定のバックアップファイル名で保存する。
pub fn backup_database_to_directory(
    connection: &Connection,
    output_directory: impl AsRef<Path>,
) -> Result<BackupResult, MaintenanceError> {
    let output_path = output_directory.as_ref().join(format!(
        "{BACKUP_FILE_PREFIX}{}.db",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    backup_database(connection, output_path)
}

fn backup_database_inner(
    connection: &Connection,
    output_path: &Path,
) -> Result<(), MaintenanceError> {
    let mut destination = Connection::open(output_path)?;
    let backup = Backup::new(connection, &mut destination)?;
    backup.run_to_completion(16, Duration::from_millis(10), None)?;
    Ok(())
}

/// shortcut_eventsとshortcut_dailyだけを即時transactionで削除する。
pub fn clear_all_usage_data(connection: &mut Connection) -> Result<ClearResult, MaintenanceError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let events_deleted = transaction.execute("DELETE FROM shortcut_events", [])? as u64;
    let daily_deleted = transaction.execute("DELETE FROM shortcut_daily", [])? as u64;
    let usage_deleted = transaction.execute("DELETE FROM application_usage_daily", [])? as u64;
    transaction.commit()?;
    Ok(ClearResult {
        events_deleted,
        daily_deleted,
        usage_deleted,
    })
}

pub fn preview_delete_on_or_before(
    connection: &Connection,
    cutoff: NaiveDate,
) -> Result<DeleteBeforePreview, MaintenanceError> {
    let cutoff_text = cutoff.format("%Y-%m-%d").to_string();
    let event_cutoff = format!("{cutoff_text} 23:59:59.999");
    let events: i64 = connection.query_row(
        "SELECT COUNT(*) FROM shortcut_events WHERE occurred_at <= ?1",
        [&event_cutoff],
        |r| r.get(0),
    )?;
    let daily_rows: i64 = connection.query_row(
        "SELECT COUNT(*) FROM shortcut_daily WHERE date <= ?1",
        [&cutoff_text],
        |r| r.get(0),
    )?;
    let usage_rows: i64 = connection.query_row(
        "SELECT COUNT(*) FROM application_usage_daily WHERE date <= ?1",
        [&cutoff_text],
        |r| r.get(0),
    )?;
    Ok(DeleteBeforePreview {
        cutoff_date: cutoff_text,
        events: events as u64,
        daily_rows: daily_rows as u64,
        usage_rows: usage_rows as u64,
    })
}

pub fn delete_on_or_before(
    connection: &mut Connection,
    cutoff: NaiveDate,
) -> Result<DeleteBeforePreview, MaintenanceError> {
    let preview = preview_delete_on_or_before(connection, cutoff)?;
    let cutoff_text = preview.cutoff_date.clone();
    let event_cutoff = format!("{cutoff_text} 23:59:59.999");
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "DELETE FROM shortcut_events WHERE occurred_at <= ?1",
        [&event_cutoff],
    )?;
    tx.execute(
        "DELETE FROM shortcut_daily WHERE date <= ?1",
        [&cutoff_text],
    )?;
    tx.execute(
        "DELETE FROM application_usage_daily WHERE date <= ?1",
        [&cutoff_text],
    )?;
    tx.commit()?;
    Ok(preview)
}

fn write_export_file<F>(path: &Path, write_rows: F) -> Result<u64, MaintenanceError>
where
    F: FnOnce(&mut BufWriter<File>) -> Result<u64, MaintenanceError>,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(&[0xEF, 0xBB, 0xBF])?;
    let result = write_rows(&mut writer);
    if result.is_err() {
        let _ = fs::remove_file(path);
        return result;
    }
    writer.flush()?;
    result
}

fn write_csv_record<'a, I>(writer: &mut impl Write, fields: I) -> Result<(), io::Error>
where
    I: IntoIterator<Item = &'a str>,
{
    for (index, field) in fields.into_iter().enumerate() {
        if index > 0 {
            writer.write_all(b",")?;
        }
        write_csv_field(writer, field)?;
    }
    writer.write_all(b"\r\n")
}

fn write_csv_field(writer: &mut impl Write, field: &str) -> Result<(), io::Error> {
    if field
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\r' | b'\n'))
    {
        writer.write_all(b"\"")?;
        for (index, part) in field.split('"').enumerate() {
            if index > 0 {
                writer.write_all(b"\"\"")?;
            }
            writer.write_all(part.as_bytes())?;
        }
        writer.write_all(b"\"")?;
    } else {
        writer.write_all(field.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};
    use rusqlite::Connection;
    use shortcut_core::model::RecordableEvent;
    use shortcut_storage::migration::run_migrations;
    use shortcut_storage::record_event;
    use shortcut_storage::repositories::excluded_app_repository::seed_default_excluded_apps;
    use shortcut_storage::repositories::settings_repository::seed_default_settings;

    fn fixture_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_settings(&connection).expect("settings");
        seed_default_excluded_apps(&connection).expect("excluded apps");
        for (minute, shortcut, application) in
            [(0, "Ctrl+C", "Code.exe"), (1, "Ctrl+V", "Code,Test.exe")]
        {
            record_event(
                &mut connection,
                &RecordableEvent {
                    occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                        .expect("date")
                        .and_hms_opt(9, minute, 0)
                        .expect("time"),
                    shortcut: shortcut.to_owned(),
                    application: application.to_owned(),
                },
            )
            .expect("record event");
        }
        connection
    }

    fn temp_directory(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shortcut-key-counter-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    fn fixed_timestamp() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 8, 15, 12, 34, 56)
            .single()
            .expect("fixed local time")
    }

    #[test]
    fn daily_csv_has_bom_header_filename_and_escaped_content() {
        let connection = fixture_connection();
        let directory = temp_directory("daily-csv");
        let result = export_daily_csv_at(&connection, &directory, fixed_timestamp(), None).expect("CSV");
        assert_eq!(result.rows, 2);
        assert_eq!(
            result.path.file_name().and_then(|name| name.to_str()),
            Some("ShortcutKeyCounter_daily_20260815_123456.csv")
        );
        let bytes = fs::read(&result.path).expect("CSV bytes");
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let content = String::from_utf8(bytes[3..].to_vec()).expect("UTF-8");
        assert!(content.starts_with("date,shortcut,application,count\r\n"));
        assert!(content.contains("2026-08-15,Ctrl+V,\"Code,Test.exe\",1\r\n"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn daily_csv_filters_selected_applications() {
        let connection = fixture_connection();
        let directory = temp_directory("daily-csv-filter");
        let selected = vec!["Code.exe".to_owned()];
        let result = export_daily_csv_at(
            &connection,
            &directory,
            fixed_timestamp(),
            Some(&selected),
        )
        .expect("filtered CSV");
        assert_eq!(result.rows, 1);
        let content = String::from_utf8(fs::read(&result.path).expect("CSV bytes")[3..].to_vec())
            .expect("UTF-8");
        assert!(content.contains("Code.exe"));
        assert!(!content.contains("Code,Test.exe"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn events_csv_contains_current_events_only() {
        let connection = fixture_connection();
        let directory = temp_directory("events-csv");
        let result = export_events_csv_at(&connection, &directory, fixed_timestamp()).expect("CSV");
        assert_eq!(result.rows, 2);
        let content = String::from_utf8(fs::read(&result.path).expect("CSV bytes")[3..].to_vec())
            .expect("UTF-8");
        assert!(content.starts_with("timestamp,shortcut,application\r\n"));
        assert!(content.contains("2026-08-15 09:00:00.000,Ctrl+C,Code.exe\r\n"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn downloads_resolution_uses_fallback_only_on_failure() {
        let fallback = PathBuf::from(r"C:\picked\directory");
        assert_eq!(
            resolve_export_directory(Ok(PathBuf::from(r"C:\Downloads")), Some(fallback.clone()))
                .expect("known folder"),
            PathBuf::from(r"C:\Downloads")
        );
        assert_eq!(
            resolve_export_directory(
                Err(KnownFoldersError::DownloadsEmpty),
                Some(fallback.clone())
            )
            .expect("fallback"),
            fallback
        );
        assert!(matches!(
            resolve_export_directory(Err(KnownFoldersError::DownloadsEmpty), None),
            Err(MaintenanceError::DownloadsUnavailable(_))
        ));
    }

    #[test]
    fn backup_opens_as_independent_database() {
        let connection = fixture_connection();
        let directory = temp_directory("backup");
        let output = directory.join("ShortcutKeyCounter_DB_20260815_123456.db");
        backup_database(&connection, &output).expect("backup");

        let backup = Connection::open(&output).expect("open backup");
        let event_count: i64 = backup
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        let daily_count: i64 = backup
            .query_row("SELECT COUNT(*) FROM shortcut_daily", [], |row| row.get(0))
            .expect("daily count");
        assert_eq!(event_count, 2);
        assert_eq!(daily_count, 2);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn clear_removes_usage_but_preserves_settings_and_exclusions() {
        let mut connection = fixture_connection();
        let result = clear_all_usage_data(&mut connection).expect("clear");
        assert_eq!(result.events_deleted, 2);
        assert_eq!(result.daily_deleted, 2);
        assert_eq!(result.usage_deleted, 0);

        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("events");
        let daily: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_daily", [], |row| row.get(0))
            .expect("daily");
        let settings: i64 = connection
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .expect("settings");
        let excluded: i64 = connection
            .query_row("SELECT COUNT(*) FROM excluded_apps", [], |row| row.get(0))
            .expect("excluded apps");
        assert_eq!(events, 0);
        assert_eq!(daily, 0);
        assert_eq!(settings, 8);
        assert_eq!(excluded, 11);
    }
}
