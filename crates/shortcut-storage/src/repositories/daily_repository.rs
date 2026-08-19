use rusqlite::{params, Transaction};
use shortcut_core::model::RecordableEvent;

use crate::{format_date, StorageError};

/// 日次集計をtransactionへ追加または加算する。
pub fn upsert_daily(
    transaction: &Transaction<'_>,
    event: &RecordableEvent,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO shortcut_daily (date, shortcut, application, count)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(date, shortcut, application)
         DO UPDATE SET count = shortcut_daily.count + 1",
        params![
            format_date(event.occurred_at),
            &event.shortcut,
            &event.application
        ],
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
    fn same_date_shortcut_and_application_is_incremented() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let event = RecordableEvent {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("date")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("time"),
            shortcut: "Ctrl+C".to_owned(),
            application: "Code.exe".to_owned(),
        };

        {
            let transaction = connection.transaction().expect("transaction");
            upsert_daily(&transaction, &event).expect("first upsert");
            transaction.commit().expect("first commit");
        }
        {
            let transaction = connection.transaction().expect("transaction");
            upsert_daily(&transaction, &event).expect("second upsert");
            transaction.commit().expect("second commit");
        }

        let count: i64 = connection
            .query_row("SELECT count FROM shortcut_daily", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 2);
    }
}
