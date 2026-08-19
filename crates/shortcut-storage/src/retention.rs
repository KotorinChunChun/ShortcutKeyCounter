use chrono::{Duration, NaiveDateTime};
use rusqlite::Connection;

use crate::{format_timestamp, StorageError};

/// nowより指定保持日数より前の詳細イベントだけを削除する。
///
/// 日次集計は削除せず、起動時の集計反映後に呼び出すことを前提とする。
pub fn cleanup_expired_events(
    connection: &Connection,
    now: NaiveDateTime,
) -> Result<usize, StorageError> {
    cleanup_expired_events_with_days(connection, now, 7)
}

pub fn cleanup_expired_events_with_days(
    connection: &Connection,
    now: NaiveDateTime,
    retention_days: i64,
) -> Result<usize, StorageError> {
    let retention_days = retention_days.clamp(1, 365);
    let cutoff = now - Duration::days(retention_days);
    let deleted = connection.execute(
        "DELETE FROM shortcut_events WHERE occurred_at < ?1",
        [format_timestamp(cutoff)],
    )?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rusqlite::Connection;
    use shortcut_core::model::RecordableEvent;

    use crate::migration::run_migrations;
    use crate::record_event;

    fn event(at: NaiveDateTime, shortcut: &str) -> RecordableEvent {
        RecordableEvent {
            occurred_at: at,
            shortcut: shortcut.to_owned(),
            application: "Code.exe".to_owned(),
        }
    }

    #[test]
    fn cleanup_removes_only_events_older_than_seven_days() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let now = NaiveDate::from_ymd_opt(2026, 8, 15)
            .expect("date")
            .and_hms_milli_opt(12, 0, 0, 0)
            .expect("time");

        record_event(&mut connection, &event(now - Duration::days(8), "Ctrl+1"))
            .expect("old event");
        record_event(
            &mut connection,
            &event(now - Duration::days(7) + Duration::minutes(1), "Ctrl+2"),
        )
        .expect("boundary event");
        record_event(&mut connection, &event(now, "Ctrl+3")).expect("today event");

        let deleted = cleanup_expired_events_with_days(&connection, now, 7).expect("cleanup");
        let event_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        let daily_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_daily", [], |row| row.get(0))
            .expect("daily count");

        assert_eq!(deleted, 1);
        assert_eq!(event_count, 2);
        assert_eq!(daily_count, 3);
    }

    #[test]
    fn custom_retention_days_are_respected() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        let now = NaiveDate::from_ymd_opt(2026, 8, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        record_event(&mut connection, &event(now - Duration::days(31), "Ctrl+1")).unwrap();
        record_event(
            &mut connection,
            &event(now - Duration::days(30) + Duration::minutes(1), "Ctrl+2"),
        )
        .unwrap();
        assert_eq!(
            cleanup_expired_events_with_days(&connection, now, 30).unwrap(),
            1
        );
    }
}
