use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::StorageError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplicationUsageRow {
    pub application: String,
    pub active_seconds: i64,
}

pub fn add_active_seconds(
    connection: &Connection,
    date: NaiveDate,
    application: &str,
    active_seconds: i64,
) -> Result<(), StorageError> {
    if active_seconds <= 0 || application.trim().is_empty() {
        return Ok(());
    }
    connection.execute(
        "INSERT INTO application_usage_daily(date, application, active_seconds)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(date, application) DO UPDATE SET
           active_seconds = application_usage_daily.active_seconds + excluded.active_seconds",
        params![
            date.format("%Y-%m-%d").to_string(),
            application,
            active_seconds
        ],
    )?;
    Ok(())
}

pub fn get_application_usage(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<ApplicationUsageRow>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT application, SUM(active_seconds)
         FROM application_usage_daily
         WHERE date >= ?1 AND date <= ?2
         GROUP BY application
         ORDER BY SUM(active_seconds) DESC, application ASC",
    )?;
    let rows = statement.query_map(
        params![
            start.format("%Y-%m-%d").to_string(),
            end.format("%Y-%m-%d").to_string()
        ],
        |row| {
            Ok(ApplicationUsageRow {
                application: row.get(0)?,
                active_seconds: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    #[test]
    fn usage_is_aggregated_per_day_and_application() {
        let mut connection = Connection::open_in_memory().unwrap();
        run_migrations(&mut connection).unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 17).unwrap();
        add_active_seconds(&connection, date, "Code.exe", 10).unwrap();
        add_active_seconds(&connection, date, "Code.exe", 20).unwrap();
        let rows = get_application_usage(&connection, date, date).unwrap();
        assert_eq!(rows[0].active_seconds, 30);
    }
}
