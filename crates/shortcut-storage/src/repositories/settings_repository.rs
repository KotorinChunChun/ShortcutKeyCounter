use rusqlite::{params, Connection};

use crate::StorageError;

pub const AUTO_START_KEY: &str = "auto_start";
pub const CAPTURE_ELEVATED_KEY: &str = "capture_elevated";
pub const THEME_KEY: &str = "theme";
pub const OVERLAY_ENABLED_KEY: &str = "overlay_enabled";
pub const USAGE_TRACKING_ENABLED_KEY: &str = "usage_tracking_enabled";
pub const DUPLICATE_GUARD_SECONDS_KEY: &str = "duplicate_guard_seconds";
pub const DETAIL_EVENT_RETENTION_DAYS_KEY: &str = "detail_event_retention_days";
pub const USAGE_GRAPH_COLORS_KEY: &str = "usage_graph_colors";
pub const DAILY_EXPORT_APPLICATIONS_KEY: &str = "daily_export_applications";

/// 初回起動時の設定を追加する。既存値は変更しない。
pub fn seed_default_settings(connection: &Connection) -> Result<(), StorageError> {
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, 'true')",
        [AUTO_START_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, 'false')",
        [CAPTURE_ELEVATED_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, 'dark')",
        [THEME_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, 'true')",
        [OVERLAY_ENABLED_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, 'false')",
        [USAGE_TRACKING_ENABLED_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, '1.0')",
        [DUPLICATE_GUARD_SECONDS_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, '7')",
        [DETAIL_EVENT_RETENTION_DAYS_KEY],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, '{}')",
        [USAGE_GRAPH_COLORS_KEY],
    )?;
    Ok(())
}

pub fn get_setting(connection: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    let mut statement = connection.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = statement.query(params![key])?;
    Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
}

/// 設定値を更新する。成功した場合だけ呼び出し元の状態変更を確定する。
pub fn set_setting(connection: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    use crate::migration::run_migrations;

    #[test]
    fn seed_inserts_defaults_once_and_preserves_user_values() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");

        seed_default_settings(&connection).expect("first seed");
        connection
            .execute(
                "UPDATE settings SET value = 'false' WHERE key = ?1",
                [AUTO_START_KEY],
            )
            .expect("user update");
        seed_default_settings(&connection).expect("second seed");

        assert_eq!(
            get_setting(&connection, AUTO_START_KEY).unwrap(),
            Some("false".to_owned())
        );
        assert_eq!(
            get_setting(&connection, CAPTURE_ELEVATED_KEY).unwrap(),
            Some("false".to_owned())
        );
        assert_eq!(
            get_setting(&connection, THEME_KEY).unwrap(),
            Some("dark".to_owned())
        );
        assert_eq!(
            get_setting(&connection, OVERLAY_ENABLED_KEY).unwrap(),
            Some("true".to_owned())
        );
        assert_eq!(
            get_setting(&connection, USAGE_TRACKING_ENABLED_KEY).unwrap(),
            Some("false".to_owned())
        );
        assert_eq!(
            get_setting(&connection, DUPLICATE_GUARD_SECONDS_KEY).unwrap(),
            Some("1.0".to_owned())
        );
        assert_eq!(
            get_setting(&connection, DETAIL_EVENT_RETENTION_DAYS_KEY).unwrap(),
            Some("7".to_owned())
        );
        assert_eq!(
            get_setting(&connection, USAGE_GRAPH_COLORS_KEY).unwrap(),
            Some("{}".to_owned())
        );
    }

    #[test]
    fn set_setting_updates_existing_value_and_inserts_new_key() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");

        set_setting(&connection, AUTO_START_KEY, "false").expect("insert setting");
        set_setting(&connection, AUTO_START_KEY, "true").expect("update setting");
        assert_eq!(
            get_setting(&connection, AUTO_START_KEY).expect("get setting"),
            Some("true".to_owned())
        );
    }
}
