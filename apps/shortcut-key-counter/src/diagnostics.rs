//! プライバシー制約付きの診断ログ。

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Local, NaiveDate};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiagnosticLogError {
    #[error("診断ログディレクトリを作成できません: {0}")]
    Directory(#[source] io::Error),
    #[error("診断ログを開けません: {0}")]
    Open(#[source] io::Error),
    #[error("診断ログを書き込めません: {0}")]
    Write(#[source] io::Error),
    #[error("診断ログの状態が壊れています")]
    StatePoisoned,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticEvent {
    AppStarted { version: String },
    AppStopped,
    DatabaseOpened,
    DatabaseOpenFailed,
    MigrationCompleted { schema_version: u32 },
    MigrationFailed,
    CleanupCompleted { deleted_events: u64 },
    HookRegistered { kind: HookKind },
    HookUnregistered { kind: HookKind },
    HookFailed { kind: HookKind },
    QueueOverflow { dropped_count: u64 },
    PowerResumed,
    PipeConnected,
    PipeDisconnected,
    HelperStartSucceeded,
    HelperStartFailed,
    ExportSucceeded { kind: ExportKind, rows: u64 },
    ExportFailed { kind: ExportKind },
    BackupSucceeded,
    BackupFailed,
    WindowsError { code: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookKind {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportKind {
    DailyCsv,
    EventsCsv,
}

struct LoggerState {
    directory: PathBuf,
    date: NaiveDate,
    writer: BufWriter<File>,
}

/// 日付が変わると新しいファイルへ切り替えるログwriter。
pub struct DiagnosticLogger {
    state: Mutex<LoggerState>,
}

impl DiagnosticLogger {
    pub fn new(directory: impl AsRef<Path>) -> Result<Self, DiagnosticLogError> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory).map_err(DiagnosticLogError::Directory)?;
        let date = Local::now().date_naive();
        let writer = open_log_file(&directory, date)?;
        Ok(Self {
            state: Mutex::new(LoggerState {
                directory,
                date,
                writer,
            }),
        })
    }

    pub fn record(&self, event: DiagnosticEvent) -> Result<(), DiagnosticLogError> {
        let now = Local::now();
        self.record_at(now, event)
    }

    fn record_at(
        &self,
        timestamp: DateTime<Local>,
        event: DiagnosticEvent,
    ) -> Result<(), DiagnosticLogError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DiagnosticLogError::StatePoisoned)?;
        let date = timestamp.date_naive();
        if date != state.date {
            state.writer = open_log_file(&state.directory, date)?;
            state.date = date;
        }
        writeln!(
            state.writer,
            "{}\t{}",
            timestamp.format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
            event.as_line()
        )
        .map_err(DiagnosticLogError::Write)?;
        state.writer.flush().map_err(DiagnosticLogError::Write)
    }
}

impl DiagnosticEvent {
    fn as_line(&self) -> String {
        match self {
            Self::AppStarted { version } => format!("app_started version={version}"),
            Self::AppStopped => "app_stopped".to_owned(),
            Self::DatabaseOpened => "database_opened".to_owned(),
            Self::DatabaseOpenFailed => "database_open_failed".to_owned(),
            Self::MigrationCompleted { schema_version } => {
                format!("migration_completed schema_version={schema_version}")
            }
            Self::MigrationFailed => "migration_failed".to_owned(),
            Self::CleanupCompleted { deleted_events } => {
                format!("cleanup_completed deleted_events={deleted_events}")
            }
            Self::HookRegistered { kind } => format!("hook_registered kind={kind:?}"),
            Self::HookUnregistered { kind } => format!("hook_unregistered kind={kind:?}"),
            Self::HookFailed { kind } => format!("hook_failed kind={kind:?}"),
            Self::QueueOverflow { dropped_count } => {
                format!("queue_overflow dropped_count={dropped_count}")
            }
            Self::PowerResumed => "power_resumed".to_owned(),
            Self::PipeConnected => "pipe_connected".to_owned(),
            Self::PipeDisconnected => "pipe_disconnected".to_owned(),
            Self::HelperStartSucceeded => "helper_start_succeeded".to_owned(),
            Self::HelperStartFailed => "helper_start_failed".to_owned(),
            Self::ExportSucceeded { kind, rows } => {
                format!("export_succeeded kind={kind:?} rows={rows}")
            }
            Self::ExportFailed { kind } => format!("export_failed kind={kind:?}"),
            Self::BackupSucceeded => "backup_succeeded".to_owned(),
            Self::BackupFailed => "backup_failed".to_owned(),
            Self::WindowsError { code } => format!("windows_error code={code}"),
        }
    }
}

fn open_log_file(directory: &Path, date: NaiveDate) -> Result<BufWriter<File>, DiagnosticLogError> {
    let path = directory.join(format!("ShortcutKeyCounter_{}.log", date.format("%Y%m%d")));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(DiagnosticLogError::Open)?;
    Ok(BufWriter::new(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("shortcut-key-counter-log-test-{unique}"))
    }

    fn fixed_local_time(year: i32, month: u32, day: u32) -> DateTime<Local> {
        chrono::Local
            .with_ymd_and_hms(year, month, day, 12, 0, 0)
            .single()
            .expect("local time")
    }

    #[test]
    fn logger_rotates_daily_and_writes_only_diagnostic_fields() {
        let directory = temp_directory();
        let logger = DiagnosticLogger::new(&directory).expect("logger");
        logger
            .record_at(
                fixed_local_time(2026, 8, 15),
                DiagnosticEvent::QueueOverflow { dropped_count: 3 },
            )
            .expect("first log");
        logger
            .record_at(
                fixed_local_time(2026, 8, 16),
                DiagnosticEvent::ExportSucceeded {
                    kind: ExportKind::DailyCsv,
                    rows: 2,
                },
            )
            .expect("rotated log");

        let first = fs::read_to_string(directory.join("ShortcutKeyCounter_20260815.log"))
            .expect("first log file");
        let second = fs::read_to_string(directory.join("ShortcutKeyCounter_20260816.log"))
            .expect("second log file");
        assert!(first.contains("queue_overflow dropped_count=3"));
        assert!(second.contains("export_succeeded kind=DailyCsv rows=2"));
        assert!(!first.contains("Ctrl+C"));
        assert!(!first.contains("Code.exe"));
        assert!(!second.contains("shortcut"));
        let _ = fs::remove_dir_all(directory);
    }
}
