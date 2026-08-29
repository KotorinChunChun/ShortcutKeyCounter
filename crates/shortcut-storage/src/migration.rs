use rusqlite::Connection;

use crate::StorageError;

const INITIAL_SCHEMA: &str = include_str!("../../../migrations/0001_initial.sql");
const HOURLY_SUMMARY_MIGRATION: &str =
    include_str!("../../../migrations/0002_hourly_summary.sql");
const BASE_SCHEMA_VERSION: i64 = 2;
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

/// SQLite schemaを現在versionまで順番に適用する。
///
/// 初回作成時は、v0.1.4までの初期schemaをversion 2として確定した後、
/// v0.1.5の時間別集計migrationを適用する。既存DBのデータと設定は削除しない。
pub fn run_migrations(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(INITIAL_SCHEMA)?;

    let mut version: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(schema_version), 0) FROM schema_meta",
        [],
        |row| row.get(0),
    )?;
    if version == 0 {
        // 0001_initial.sql contains the complete v0.1.4 schema, including
        // application_usage_daily, so it is the v2 baseline for a new DB.
        transaction.execute(
            "INSERT INTO schema_meta(schema_version) VALUES (?1)",
            [BASE_SCHEMA_VERSION],
        )?;
        version = BASE_SCHEMA_VERSION;
    }

    if version < i64::from(CURRENT_SCHEMA_VERSION) {
        if version < BASE_SCHEMA_VERSION {
            // There is no older schema in the supported v0.1.5 upgrade path;
            // applying the complete initial schema above still gives such a
            // database the known baseline before continuing.
            version = BASE_SCHEMA_VERSION;
        }
        if version < 3 {
            transaction.execute_batch(HOURLY_SUMMARY_MIGRATION)?;
            version = 3;
        }
        transaction.execute("DELETE FROM schema_meta", [])?;
        transaction.execute(
            "INSERT INTO schema_meta(schema_version) VALUES (?1)",
            [version],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_seven_tables_and_indexes() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");

        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_%'",
                [],
                |row| row.get(0),
            )
            .expect("index count");
        let version: i64 = connection
            .query_row("SELECT schema_version FROM schema_meta", [], |row| {
                row.get(0)
            })
            .expect("schema version");

        assert_eq!(table_count, 7);
        assert_eq!(index_count, 9);
        assert_eq!(version, 3);
    }

    #[test]
    fn migration_preserves_existing_rows_and_backfills_event_hours_only() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        connection
            .execute_batch(INITIAL_SCHEMA)
            .expect("legacy schema");
        connection
            .execute("INSERT INTO schema_meta(schema_version) VALUES (2)", [])
            .expect("legacy version");
        connection
            .execute(
                "INSERT INTO shortcut_events(occurred_at, shortcut, application)
                 VALUES ('2026-08-15 09:14:21.152', 'Ctrl+C', 'Code.exe')",
                [],
            )
            .expect("event");
        connection
            .execute(
                "INSERT INTO shortcut_daily(date, shortcut, application, count)
                 VALUES ('2026-08-16', 'Ctrl+V', 'Code.exe', 4)",
                [],
            )
            .expect("daily-only row");

        run_migrations(&mut connection).expect("migration");

        let event_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        let daily_count: i64 = connection
            .query_row("SELECT count FROM shortcut_daily WHERE date = '2026-08-16'", [], |row| {
                row.get(0)
            })
            .expect("daily-only count");
        let hourly: (String, i64, i64) = connection
            .query_row(
                "SELECT date, hour, count FROM shortcut_hourly_summary",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("hourly row");
        let daily_only_hourly: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM shortcut_hourly_summary WHERE date = '2026-08-16'",
                [],
                |row| row.get(0),
            )
            .expect("daily-only hourly count");

        assert_eq!(event_count, 1);
        assert_eq!(daily_count, 4);
        assert_eq!(hourly, ("2026-08-15".to_owned(), 9, 1));
        assert_eq!(daily_only_hourly, 0);
    }

    #[test]
    fn migration_is_idempotent_and_does_not_reset_metadata_or_data() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("first migration");
        connection
            .execute(
                "INSERT INTO settings(key, value) VALUES ('migration-test', 'keep')",
                [],
            )
            .expect("setting");
        run_migrations(&mut connection).expect("second migration");

        let version: i64 = connection
            .query_row("SELECT schema_version FROM schema_meta", [], |row| row.get(0))
            .expect("schema version");
        let value: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'migration-test'",
                [],
                |row| row.get(0),
            )
            .expect("setting value");
        assert_eq!(version, i64::from(CURRENT_SCHEMA_VERSION));
        assert_eq!(value, "keep");
    }
}
