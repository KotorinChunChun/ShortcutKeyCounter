use rusqlite::Connection;

use crate::StorageError;

const INITIAL_SCHEMA: &str = include_str!("../../../migrations/0001_initial.sql");

/// 空のDBへschema v2を適用する。
pub fn run_migrations(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(INITIAL_SCHEMA)?;
    transaction.execute("DELETE FROM schema_meta", [])?;
    transaction.execute(
        "INSERT INTO schema_meta(schema_version) VALUES (?1)",
        [2_i64],
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_six_tables_and_indexes() {
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

        assert_eq!(table_count, 6);
        assert_eq!(index_count, 8);
        assert_eq!(version, 2);
    }
}
