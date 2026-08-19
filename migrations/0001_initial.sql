PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS shortcut_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at TEXT NOT NULL,
    shortcut TEXT NOT NULL,
    application TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS shortcut_daily (
    date TEXT NOT NULL,
    shortcut TEXT NOT NULL,
    application TEXT NOT NULL,
    count INTEGER NOT NULL CHECK (count >= 0),
    UNIQUE (date, shortcut, application)
);

CREATE TABLE IF NOT EXISTS excluded_apps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    executable_name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    source TEXT NOT NULL CHECK (source IN ('default', 'user')),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_meta (
    schema_version INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_occurred_at
    ON shortcut_events (occurred_at);

CREATE INDEX IF NOT EXISTS idx_events_shortcut_occurred
    ON shortcut_events (shortcut, occurred_at);

CREATE INDEX IF NOT EXISTS idx_events_application_occurred
    ON shortcut_events (application, occurred_at);

CREATE INDEX IF NOT EXISTS idx_daily_date
    ON shortcut_daily (date);

CREATE INDEX IF NOT EXISTS idx_daily_shortcut_date
    ON shortcut_daily (shortcut, date);

CREATE INDEX IF NOT EXISTS idx_daily_application_date
    ON shortcut_daily (application, date);

CREATE TABLE IF NOT EXISTS application_usage_daily (
    date TEXT NOT NULL,
    application TEXT NOT NULL,
    active_seconds INTEGER NOT NULL CHECK (active_seconds >= 0),
    UNIQUE (date, application)
);

CREATE INDEX IF NOT EXISTS idx_application_usage_daily_date
    ON application_usage_daily (date);
CREATE INDEX IF NOT EXISTS idx_application_usage_daily_application_date
    ON application_usage_daily (application, date);
