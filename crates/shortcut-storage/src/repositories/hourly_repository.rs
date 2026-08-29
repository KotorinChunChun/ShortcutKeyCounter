use chrono::Timelike;
use rusqlite::{params, Transaction};
use shortcut_core::model::RecordableEvent;

use crate::{format_date, StorageError};

/// 時間別集計を同一transactionへ追加または加算する。
pub fn upsert_hourly(
    transaction: &Transaction<'_>,
    event: &RecordableEvent,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO shortcut_hourly_summary (date, hour, count)
         VALUES (?1, ?2, 1)
         ON CONFLICT(date, hour)
         DO UPDATE SET count = shortcut_hourly_summary.count + 1",
        params![format_date(event.occurred_at), i64::from(event.occurred_at.hour())],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rusqlite::Connection;

    use crate::migration::run_migrations;

    #[test]
    fn same_date_and_hour_is_incremented() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let event = RecordableEvent {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("date")
                .and_hms_opt(9, 14, 21)
                .expect("time"),
            shortcut: "Ctrl+C".to_owned(),
            application: "Code.exe".to_owned(),
        };

        for _ in 0..2 {
            let transaction = connection.transaction().expect("transaction");
            upsert_hourly(&transaction, &event).expect("upsert");
            transaction.commit().expect("commit");
        }

        let row: (String, i64, i64) = connection
            .query_row(
                "SELECT date, hour, count FROM shortcut_hourly_summary",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("hourly row");
        assert_eq!(row, ("2026-08-15".to_owned(), 9, 2));
    }
}
