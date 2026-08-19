//! SQLite永続化と集計を提供するクレート。

use chrono::NaiveDateTime;
use thiserror::Error;

pub mod application_usage;
pub mod db;
pub mod migration;
pub mod recorder;
pub mod repositories;
pub mod retention;
pub mod statistics;

/// Storage層で発生したエラー。
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLiteエラー: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("ファイルシステムエラー: {0}")]
    Io(#[from] std::io::Error),
    #[error("日付範囲が不正です: 開始日が終了日より後です")]
    InvalidDateRange,
    #[error("設定値が不正です: {0}")]
    InvalidSettingValue(String),
    #[error("実行ファイル名が不正です")]
    InvalidExecutableName,
}

pub(crate) fn format_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

pub(crate) fn format_date(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d").to_string()
}

pub use recorder::record_event;
pub use retention::cleanup_expired_events;
