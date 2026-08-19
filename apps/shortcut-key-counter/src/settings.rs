//! 永続設定とWindows実状態の同期。

use std::path::Path;

use shortcut_storage::repositories::settings_repository::{
    get_setting, set_setting, AUTO_START_KEY, CAPTURE_ELEVATED_KEY,
    DETAIL_EVENT_RETENTION_DAYS_KEY, DUPLICATE_GUARD_SECONDS_KEY, OVERLAY_ENABLED_KEY, THEME_KEY,
    USAGE_GRAPH_COLORS_KEY, USAGE_TRACKING_ENABLED_KEY,
};
use shortcut_storage::StorageError;
use shortcut_windows::autostart::{
    autostart_command, disable_autostart, enable_autostart, read_autostart_command, AutostartError,
};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("設定の読み書きに失敗しました: {0}")]
    Storage(#[from] StorageError),
    #[error("自動起動のWindows同期に失敗しました: {0}")]
    Autostart(#[from] AutostartError),
    #[error("設定値がtrue/falseではありません: {0}")]
    InvalidBoolean(String),
    #[error("テーマ設定がdark/lightではありません: {0}")]
    InvalidTheme(String),
    #[error("数値設定が不正です: {0}")]
    InvalidNumber(String),
    #[error("グラフ色設定が不正です")]
    InvalidGraphColors,
}

pub fn get_auto_start(connection: &rusqlite::Connection) -> Result<bool, SettingsError> {
    get_boolean_setting(connection, AUTO_START_KEY, true)
}

pub fn get_capture_elevated(connection: &rusqlite::Connection) -> Result<bool, SettingsError> {
    get_boolean_setting(connection, CAPTURE_ELEVATED_KEY, false)
}

pub fn get_overlay_enabled(connection: &rusqlite::Connection) -> Result<bool, SettingsError> {
    get_boolean_setting(connection, OVERLAY_ENABLED_KEY, true)
}

pub fn set_overlay_enabled(
    connection: &rusqlite::Connection,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_setting(
        connection,
        OVERLAY_ENABLED_KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

pub fn get_usage_tracking_enabled(
    connection: &rusqlite::Connection,
) -> Result<bool, SettingsError> {
    get_boolean_setting(connection, USAGE_TRACKING_ENABLED_KEY, false)
}

pub fn set_usage_tracking_enabled(
    connection: &rusqlite::Connection,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_setting(
        connection,
        USAGE_TRACKING_ENABLED_KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

pub fn get_duplicate_guard_seconds(
    connection: &rusqlite::Connection,
) -> Result<f64, SettingsError> {
    match get_setting(connection, DUPLICATE_GUARD_SECONDS_KEY)? {
        None => Ok(1.0),
        Some(value) => value
            .parse::<f64>()
            .ok()
            .filter(|v| (0.0..=10.0).contains(v))
            .ok_or_else(|| SettingsError::InvalidNumber(DUPLICATE_GUARD_SECONDS_KEY.to_owned())),
    }
}

pub fn set_duplicate_guard_seconds(
    connection: &rusqlite::Connection,
    seconds: f64,
) -> Result<(), SettingsError> {
    if !seconds.is_finite() || !(0.0..=10.0).contains(&seconds) {
        return Err(SettingsError::InvalidNumber(
            DUPLICATE_GUARD_SECONDS_KEY.to_owned(),
        ));
    }
    let rounded = (seconds * 10.0).round() / 10.0;
    set_setting(
        connection,
        DUPLICATE_GUARD_SECONDS_KEY,
        &format!("{rounded:.1}"),
    )?;
    Ok(())
}

pub fn get_detail_event_retention_days(
    connection: &rusqlite::Connection,
) -> Result<u32, SettingsError> {
    match get_setting(connection, DETAIL_EVENT_RETENTION_DAYS_KEY)? {
        None => Ok(7),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=365).contains(v))
            .ok_or_else(|| {
                SettingsError::InvalidNumber(DETAIL_EVENT_RETENTION_DAYS_KEY.to_owned())
            }),
    }
}

pub fn set_detail_event_retention_days(
    connection: &rusqlite::Connection,
    days: u32,
) -> Result<(), SettingsError> {
    if !(1..=365).contains(&days) {
        return Err(SettingsError::InvalidNumber(
            DETAIL_EVENT_RETENTION_DAYS_KEY.to_owned(),
        ));
    }
    set_setting(
        connection,
        DETAIL_EVENT_RETENTION_DAYS_KEY,
        &days.to_string(),
    )?;
    Ok(())
}

pub fn get_usage_graph_colors(
    connection: &rusqlite::Connection,
) -> Result<BTreeMap<String, String>, SettingsError> {
    let raw = get_setting(connection, USAGE_GRAPH_COLORS_KEY)?.unwrap_or_else(|| "{}".to_owned());
    serde_json::from_str::<BTreeMap<String, String>>(&raw)
        .map_err(|_| SettingsError::InvalidGraphColors)
}

pub fn set_usage_graph_colors(
    connection: &rusqlite::Connection,
    colors: &BTreeMap<String, String>,
) -> Result<(), SettingsError> {
    if colors
        .iter()
        .any(|(key, value)| key.len() > 256 || !is_hex_color(value))
    {
        return Err(SettingsError::InvalidGraphColors);
    }
    let raw = serde_json::to_string(colors).map_err(|_| SettingsError::InvalidGraphColors)?;
    set_setting(connection, USAGE_GRAPH_COLORS_KEY, &raw)?;
    Ok(())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn get_theme(connection: &rusqlite::Connection) -> Result<String, SettingsError> {
    match get_setting(connection, THEME_KEY)? {
        None => Ok("dark".to_owned()),
        Some(value) if value.eq_ignore_ascii_case("dark") => Ok("dark".to_owned()),
        Some(value) if value.eq_ignore_ascii_case("light") => Ok("light".to_owned()),
        Some(value) => Err(SettingsError::InvalidTheme(value)),
    }
}

pub fn set_theme(connection: &rusqlite::Connection, theme: &str) -> Result<(), SettingsError> {
    let normalized = if theme.eq_ignore_ascii_case("dark") {
        "dark"
    } else if theme.eq_ignore_ascii_case("light") {
        "light"
    } else {
        return Err(SettingsError::InvalidTheme(theme.to_owned()));
    };
    set_setting(connection, THEME_KEY, normalized)?;
    Ok(())
}

/// Windows操作成功後だけDBのauto_start値を更新する。
pub fn set_auto_start(
    connection: &rusqlite::Connection,
    executable_path: impl AsRef<Path>,
    enabled: bool,
) -> Result<(), SettingsError> {
    if enabled {
        enable_autostart(executable_path)?;
    } else {
        disable_autostart()?;
    }
    set_setting(
        connection,
        AUTO_START_KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

pub fn set_capture_elevated(
    connection: &rusqlite::Connection,
    enabled: bool,
) -> Result<(), SettingsError> {
    set_setting(
        connection,
        CAPTURE_ELEVATED_KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

/// 起動時にauto_start設定と現在のexeパスをRunキーへ同期する。
pub fn synchronize_auto_start(
    connection: &rusqlite::Connection,
    executable_path: impl AsRef<Path>,
) -> Result<(), SettingsError> {
    let desired = get_auto_start(connection)?;
    if desired {
        let expected = autostart_command(&executable_path)?;
        if read_autostart_command()?.as_deref() != Some(expected.as_str()) {
            enable_autostart(executable_path)?;
        }
    } else if read_autostart_command()?.is_some() {
        disable_autostart()?;
    }
    Ok(())
}

fn get_boolean_setting(
    connection: &rusqlite::Connection,
    key: &str,
    default: bool,
) -> Result<bool, SettingsError> {
    match get_setting(connection, key)? {
        None => Ok(default),
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        Some(_) => Err(SettingsError::InvalidBoolean(key.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use shortcut_storage::migration::run_migrations;
    use shortcut_storage::repositories::settings_repository::seed_default_settings;

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        seed_default_settings(&connection).expect("seed");
        connection
    }

    #[test]
    fn reads_and_updates_boolean_settings_without_windows_side_effects() {
        let connection = connection();
        assert!(get_auto_start(&connection).expect("auto start"));
        assert!(!get_capture_elevated(&connection).expect("elevated"));
        assert_eq!(get_theme(&connection).expect("theme"), "dark");
        assert!(get_overlay_enabled(&connection).expect("overlay"));
        assert!(!get_usage_tracking_enabled(&connection).expect("usage"));
        assert_eq!(get_duplicate_guard_seconds(&connection).unwrap(), 1.0);
        assert_eq!(get_detail_event_retention_days(&connection).unwrap(), 7);
        set_overlay_enabled(&connection, false).expect("set overlay");
        assert!(!get_overlay_enabled(&connection).expect("overlay"));
        set_theme(&connection, "light").expect("set theme");
        assert_eq!(get_theme(&connection).expect("theme"), "light");
        set_usage_tracking_enabled(&connection, true).expect("set usage");
        assert!(get_usage_tracking_enabled(&connection).expect("usage"));
        set_capture_elevated(&connection, true).expect("set elevated");
        assert!(get_capture_elevated(&connection).expect("elevated"));
        set_duplicate_guard_seconds(&connection, 2.34).unwrap();
        assert_eq!(get_duplicate_guard_seconds(&connection).unwrap(), 2.3);
        set_detail_event_retention_days(&connection, 60).unwrap();
        assert_eq!(get_detail_event_retention_days(&connection).unwrap(), 60);
        let mut colors = BTreeMap::new();
        colors.insert("key:Ctrl+C".to_owned(), "#2563eb".to_owned());
        set_usage_graph_colors(&connection, &colors).unwrap();
        assert_eq!(get_usage_graph_colors(&connection).unwrap(), colors);
    }

    #[test]
    fn invalid_theme_is_rejected() {
        let connection = connection();
        assert!(matches!(
            set_theme(&connection, "system"),
            Err(SettingsError::InvalidTheme(_))
        ));
    }

    #[test]
    fn invalid_boolean_setting_is_rejected() {
        let connection = connection();
        set_setting(&connection, AUTO_START_KEY, "maybe").expect("set invalid value");
        assert!(matches!(
            get_auto_start(&connection),
            Err(SettingsError::InvalidBoolean(key)) if key == AUTO_START_KEY
        ));
    }
}
