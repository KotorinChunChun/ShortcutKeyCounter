use rusqlite::Connection;
use shortcut_core::model::RecordableEvent;

use crate::repositories::daily_repository::upsert_daily;
use crate::repositories::event_repository::insert_event;
use crate::repositories::hourly_repository::upsert_hourly;
use crate::StorageError;

/// 詳細イベントと日次・時間別集計を一つのtransactionで保存する。
pub fn record_event(
    connection: &mut Connection,
    event: &RecordableEvent,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    insert_event(&transaction, event)?;
    upsert_daily(&transaction, event)?;
    upsert_hourly(&transaction, event)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rusqlite::Connection;

    use crate::migration::run_migrations;

    fn test_event() -> RecordableEvent {
        RecordableEvent {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("date")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("time"),
            shortcut: "Ctrl+C".to_owned(),
            application: "Code.exe".to_owned(),
        }
    }

    #[test]
    fn event_and_daily_are_saved_atomically() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let event = test_event();

        record_event(&mut connection, &event).expect("first event");
        record_event(&mut connection, &event).expect("second event");

        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        let daily: i64 = connection
            .query_row("SELECT count FROM shortcut_daily", [], |row| row.get(0))
            .expect("daily count");
        let hourly: i64 = connection
            .query_row(
                "SELECT count FROM shortcut_hourly_summary",
                [],
                |row| row.get(0),
            )
            .expect("hourly count");

        assert_eq!(events, 2);
        assert_eq!(daily, 2);
        assert_eq!(hourly, 2);
    }

    #[test]
    fn failed_daily_write_rolls_back_event_insert() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let event = test_event();

        let result: Result<(), StorageError> = (|| {
            let transaction = connection.transaction()?;
            insert_event(&transaction, &event)?;
            transaction.execute(
                "INSERT INTO shortcut_daily(date, shortcut, application, count)
                 VALUES ('2026-08-15', 'Ctrl+C', 'Code.exe', -1)",
                [],
            )?;
            transaction.commit()?;
            Ok(())
        })();

        assert!(result.is_err());
        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(events, 0);
    }

    #[test]
    fn failed_hourly_write_rolls_back_event_and_daily_inserts() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        connection
            .execute("DROP TABLE shortcut_hourly_summary", [])
            .expect("drop hourly table");

        let result = record_event(&mut connection, &test_event());
        assert!(result.is_err());

        let events: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        let daily: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_daily", [], |row| row.get(0))
            .expect("daily count");
        assert_eq!(events, 0);
        assert_eq!(daily, 0);
    }
}
