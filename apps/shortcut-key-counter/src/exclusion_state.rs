//! 除外アプリ一覧をDBとCapture worker間で同期する。

use std::collections::HashSet;

use rusqlite::Connection;
use shortcut_storage::repositories::excluded_app_repository::load_enabled_excluded_apps;
use shortcut_storage::StorageError;
use thiserror::Error;

use crate::capture_worker::SharedExcludedApps;

#[derive(Debug, Error)]
pub enum ExclusionStateError {
    #[error("除外アプリ一覧をDBから読み込めません: {0}")]
    Storage(#[from] StorageError),
    #[error("除外アプリ共有stateが壊れています")]
    StatePoisoned,
}

/// enabledな除外アプリだけをDBから再読込し、workerが参照する集合を置き換える。
pub fn reload_enabled_exclusions(
    connection: &Connection,
    shared: &SharedExcludedApps,
) -> Result<(), ExclusionStateError> {
    let refreshed = load_enabled_excluded_apps(connection)?
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut guard = shared
        .write()
        .map_err(|_| ExclusionStateError::StatePoisoned)?;
    *guard = refreshed;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture_worker::shared_excluded_apps;
    use shortcut_storage::migration::run_migrations;
    use shortcut_storage::repositories::excluded_app_repository::{
        add_user_excluded_app, seed_default_excluded_apps,
    };

    #[test]
    fn reload_replaces_shared_set_with_current_enabled_rows() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_excluded_apps(&connection).expect("defaults");
        let shared = shared_excluded_apps(HashSet::new());

        add_user_excluded_app(&connection, "PrivateNotes.exe").expect("add");
        reload_enabled_exclusions(&connection, &shared).expect("reload");

        let guard = shared.read().expect("read");
        assert!(guard.contains("keepass.exe"));
        assert!(guard.contains("privatenotes.exe"));
    }
}
