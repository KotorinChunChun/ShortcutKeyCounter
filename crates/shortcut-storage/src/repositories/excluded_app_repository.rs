use rusqlite::{params, Connection};
use serde::Serialize;

use crate::StorageError;

pub const DEFAULT_EXCLUDED_APPS: [&str; 11] = [
    "KeePass.exe",
    "KeePassXC.exe",
    "Bitwarden.exe",
    "Enpass.exe",
    "1Password.exe",
    "NordPass.exe",
    "ProtonPass.exe",
    "KeeperPasswordManager.exe",
    "RoboForm.exe",
    "StickyPassword.exe",
    "PasswordSafe.exe",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExcludedApp {
    pub id: i64,
    pub executable_name: String,
    pub source: String,
    pub enabled: bool,
}

/// 初期除外アプリを追加する。既存のenabled値は変更しない。
pub fn seed_default_excluded_apps(connection: &Connection) -> Result<(), StorageError> {
    for executable_name in DEFAULT_EXCLUDED_APPS {
        connection.execute(
            "INSERT OR IGNORE INTO excluded_apps(executable_name, source, enabled)
             VALUES (?1, 'default', 1)",
            params![executable_name],
        )?;
    }
    Ok(())
}

/// 有効な除外アプリの実行ファイル名を全件読み出す。
pub fn load_enabled_excluded_apps(connection: &Connection) -> Result<Vec<String>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT executable_name FROM excluded_apps
         WHERE enabled = 1
         ORDER BY id",
    )?;
    let executable_names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(executable_names)
}

pub fn is_enabled_excluded(
    connection: &Connection,
    executable_name: &str,
) -> Result<bool, StorageError> {
    let enabled = connection.query_row(
        "SELECT enabled FROM excluded_apps
         WHERE executable_name = ?1 COLLATE NOCASE AND enabled = 1",
        [executable_name],
        |row| row.get::<_, i64>(0),
    );
    match enabled {
        Ok(value) => Ok(value == 1),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub fn list_excluded_apps(connection: &Connection) -> Result<Vec<ExcludedApp>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, executable_name, source, enabled
         FROM excluded_apps ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ExcludedApp {
            id: row.get(0)?,
            executable_name: row.get(1)?,
            source: row.get(2)?,
            enabled: row.get::<_, i64>(3)? == 1,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn add_user_excluded_app(
    connection: &Connection,
    executable_name: &str,
) -> Result<(), StorageError> {
    validate_executable_name(executable_name)?;
    connection.execute(
        "INSERT INTO excluded_apps(executable_name, source, enabled)
         VALUES (?1, 'user', 1)
         ON CONFLICT(executable_name) DO UPDATE SET enabled = 1",
        params![executable_name.trim()],
    )?;
    Ok(())
}

pub fn set_excluded_app_enabled(
    connection: &Connection,
    executable_name: &str,
    enabled: bool,
) -> Result<bool, StorageError> {
    validate_executable_name(executable_name)?;
    let changed = connection.execute(
        "UPDATE excluded_apps SET enabled = ?1
         WHERE executable_name = ?2 COLLATE NOCASE",
        params![i64::from(enabled), executable_name.trim()],
    )?;
    Ok(changed != 0)
}

pub fn remove_user_excluded_app(
    connection: &Connection,
    executable_name: &str,
) -> Result<bool, StorageError> {
    validate_executable_name(executable_name)?;
    let removed = connection.execute(
        "DELETE FROM excluded_apps
         WHERE executable_name = ?1 COLLATE NOCASE AND source = 'user'",
        params![executable_name.trim()],
    )?;
    Ok(removed != 0)
}

fn validate_executable_name(executable_name: &str) -> Result<(), StorageError> {
    let executable_name = executable_name.trim();
    if executable_name.is_empty()
        || executable_name.len() > 260
        || executable_name
            .chars()
            .any(|character| matches!(character, '\\' | '/' | '\0'))
    {
        return Err(StorageError::InvalidExecutableName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    use crate::migration::run_migrations;

    #[test]
    fn seed_inserts_eleven_defaults_and_preserves_disabled_value() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_excluded_apps(&connection).expect("first seed");
        connection
            .execute(
                "UPDATE excluded_apps SET enabled = 0 WHERE executable_name = ?1",
                [DEFAULT_EXCLUDED_APPS[0]],
            )
            .expect("disable app");
        seed_default_excluded_apps(&connection).expect("second seed");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM excluded_apps", [], |row| row.get(0))
            .expect("count");
        let enabled: i64 = connection
            .query_row(
                "SELECT enabled FROM excluded_apps WHERE executable_name = ?1",
                [DEFAULT_EXCLUDED_APPS[0]],
                |row| row.get(0),
            )
            .expect("enabled");

        assert_eq!(count, 11);
        assert_eq!(enabled, 0);
    }

    #[test]
    fn load_returns_eleven_enabled_defaults_and_excludes_disabled_app() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_excluded_apps(&connection).expect("seed");

        let loaded = load_enabled_excluded_apps(&connection).expect("load enabled apps");
        assert_eq!(loaded.len(), 11);

        connection
            .execute(
                "UPDATE excluded_apps SET enabled = 0 WHERE executable_name = ?1",
                [DEFAULT_EXCLUDED_APPS[0]],
            )
            .expect("disable app");

        let loaded = load_enabled_excluded_apps(&connection).expect("load enabled apps");
        assert_eq!(loaded.len(), 10);
        assert!(!loaded.iter().any(|name| name == DEFAULT_EXCLUDED_APPS[0]));
    }

    #[test]
    fn user_excluded_apps_can_be_added_disabled_and_removed_without_deleting_defaults() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_excluded_apps(&connection).expect("seed");

        add_user_excluded_app(&connection, "custom.exe").expect("add user app");
        assert!(set_excluded_app_enabled(&connection, "CUSTOM.EXE", false).expect("disable"));
        let apps = list_excluded_apps(&connection).expect("list");
        let custom = apps
            .iter()
            .find(|app| app.executable_name.eq_ignore_ascii_case("custom.exe"))
            .expect("custom app");
        assert_eq!(custom.source, "user");
        assert!(!custom.enabled);

        assert!(remove_user_excluded_app(&connection, "custom.exe").expect("remove"));
        assert!(!remove_user_excluded_app(&connection, "custom.exe").expect("remove again"));
        assert_eq!(list_excluded_apps(&connection).expect("list").len(), 11);
        assert!(!remove_user_excluded_app(&connection, DEFAULT_EXCLUDED_APPS[0]).expect("default"));
    }

    #[test]
    fn user_excluded_app_rejects_paths() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        assert!(matches!(
            add_user_excluded_app(&connection, r"C:\secret\app.exe"),
            Err(StorageError::InvalidExecutableName)
        ));
    }
}
