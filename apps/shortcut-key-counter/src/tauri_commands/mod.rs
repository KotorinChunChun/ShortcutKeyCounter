//! WebViewからRust backendへ接続するTauri command群。
//!
//! commandはドメインごとのサブモジュールに置き、ここには共通のDB lock、
//! 引数解析、エラー変換だけを置く。command名・引数・戻り値・エラーコードは
//! UIとの契約なので、モジュールを移動しても変更しない。

pub mod app;
pub mod capture;
pub mod data;
pub mod mapping;
pub mod overlay;
pub mod settings;
pub mod statistics;

use std::path::PathBuf;

use chrono::NaiveDate;

use crate::app_state::AppState;
use crate::command_api::{self, SettingsSnapshot};
use crate::command_error::CommandError;
use crate::maintenance::downloads_or_fallback;

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

fn parse_date_range(start: &str, end: &str) -> Result<(NaiveDate, NaiveDate), CommandError> {
    Ok((parse_date(start)?, parse_date(end)?))
}

/// 設定変更コマンドの共通戻り値として、保存後の設定を読み直す。
fn read_settings(db: &rusqlite::Connection) -> Result<SettingsSnapshot, CommandError> {
    command_api::get_settings(db)
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))
}

fn settings_snapshot(state: &AppState) -> Result<SettingsSnapshot, CommandError> {
    let db = settings_db_lock(state)?;
    read_settings(&db)
}

fn downloads_directory(error_code: &str) -> Result<PathBuf, CommandError> {
    downloads_or_fallback(None)
        .map_err(|_| CommandError::new(error_code, "Downloadsフォルダーを取得できませんでした。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_range_requires_iso_dates() {
        let (start, end) = parse_date_range("2026-08-01", "2026-08-31").expect("valid range");
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 8, 1).expect("date"));
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 8, 31).expect("date"));
        for (start, end) in [("2026/08/01", "2026-08-31"), ("2026-08-01", "")] {
            let error = parse_date_range(start, end).expect_err("invalid date");
            assert_eq!(error.code, "INVALID_DATE");
            assert_eq!(error.message, "日付の形式が不正です。");
        }
    }
}
