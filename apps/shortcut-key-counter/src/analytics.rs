//! v0.1.4 ranking / usage-graph aggregation.
//!
//! Frontendへ詳細ショートカット履歴全件を渡さず、SQLite側で期間・系列ごとに集計する。

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("unsupported analytics range: {0}")]
    InvalidRange(String),
    #[error("unsupported graph mode: {0}")]
    InvalidMode(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalyticsRange {
    Today,
    SevenDays,
    ThirtyDays,
    All,
}

impl AnalyticsRange {
    pub fn parse(value: &str) -> Result<Self, AnalyticsError> {
        match value {
            "today" => Ok(Self::Today),
            "7" | "7d" => Ok(Self::SevenDays),
            "30" | "30d" => Ok(Self::ThirtyDays),
            "all" => Ok(Self::All),
            other => Err(AnalyticsError::InvalidRange(other.to_owned())),
        }
    }

    pub fn comparison_label(self) -> Option<&'static str> {
        match self {
            Self::Today => Some("前日比"),
            Self::SevenDays => Some("先週比"),
            Self::ThirtyDays => Some("前月比"),
            Self::All => None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyRankingItem {
    pub shortcut: String,
    pub count: i64,
    pub average: f64,
    pub ratio: f64,
    pub comparison_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApplicationRankingItem {
    pub application: String,
    pub count: i64,
    pub key_type_count: i64,
    pub active_seconds: i64,
    pub ratio: f64,
    pub comparison_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RankingsSnapshot {
    pub comparison_label: Option<String>,
    pub applications: Vec<String>,
    pub key_rows: Vec<KeyRankingItem>,
    pub application_rows: Vec<ApplicationRankingItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageGraphPoint {
    pub bucket: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageGraphSeries {
    pub identity: String,
    pub points: Vec<UsageGraphPoint>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageGraphSnapshot {
    pub bucket_kind: String,
    pub buckets: Vec<String>,
    pub series: Vec<UsageGraphSeries>,
}

#[derive(Clone, Debug)]
struct Period {
    start_date: NaiveDate,
    end_date: NaiveDate,
    start_at: Option<NaiveDateTime>,
    end_at: Option<NaiveDateTime>,
    average_days: f64,
}

pub fn rankings(
    connection: &Connection,
    range: AnalyticsRange,
    application_filter: Option<&str>,
    now: NaiveDateTime,
) -> Result<RankingsSnapshot, AnalyticsError> {
    let current = current_period(connection, range, now)?;
    let previous = previous_period(range, &current, now);
    let applications = list_applications(connection)?;

    let current_keys = key_counts(connection, &current, application_filter)?;
    let key_average_days = if range == AnalyticsRange::All {
        distinct_data_days_filtered(
            connection,
            current.start_date,
            current.end_date,
            application_filter,
        )?
        .max(1) as f64
    } else {
        current.average_days
    };
    let previous_keys = match previous.as_ref() {
        Some(period) => key_counts(connection, period, application_filter)?,
        None => HashMap::new(),
    };
    let key_total: i64 = current_keys.values().sum();
    let mut key_rows = current_keys
        .into_iter()
        .map(|(shortcut, count)| KeyRankingItem {
            shortcut: shortcut.clone(),
            count,
            average: count as f64 / key_average_days.max(1.0),
            ratio: percent(count, key_total),
            comparison_rate: previous.as_ref().and_then(|_| {
                change_rate(count, previous_keys.get(&shortcut).copied().unwrap_or(0))
            }),
        })
        .collect::<Vec<_>>();
    key_rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.shortcut.cmp(&b.shortcut))
    });

    let current_apps = application_counts(connection, &current)?;
    let previous_apps = match previous.as_ref() {
        Some(period) => application_counts(connection, period)?,
        None => HashMap::new(),
    };
    let app_total: i64 = current_apps.values().sum();
    let app_key_types = application_key_type_counts(connection, &current)?;
    let active_seconds = application_active_seconds(connection, &current)?;
    let mut application_rows = current_apps
        .into_iter()
        .map(|(application, count)| ApplicationRankingItem {
            application: application.clone(),
            count,
            key_type_count: app_key_types.get(&application).copied().unwrap_or(0),
            active_seconds: active_seconds.get(&application).copied().unwrap_or(0),
            ratio: percent(count, app_total),
            comparison_rate: previous.as_ref().and_then(|_| {
                change_rate(count, previous_apps.get(&application).copied().unwrap_or(0))
            }),
        })
        .collect::<Vec<_>>();
    application_rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.active_seconds.cmp(&a.active_seconds))
            .then_with(|| a.application.cmp(&b.application))
    });

    Ok(RankingsSnapshot {
        comparison_label: range.comparison_label().map(str::to_owned),
        applications,
        key_rows,
        application_rows,
    })
}

pub fn usage_graph(
    connection: &Connection,
    mode: &str,
    range: AnalyticsRange,
    identities: &[String],
    now: NaiveDateTime,
) -> Result<UsageGraphSnapshot, AnalyticsError> {
    let column = match mode {
        "keys" | "key" => "shortcut",
        "apps" | "app" => "application",
        other => return Err(AnalyticsError::InvalidMode(other.to_owned())),
    };
    let period = current_period(connection, range, now)?;
    let (bucket_kind, buckets) = buckets_for(range, &period, now);
    let mut series = Vec::with_capacity(identities.len().min(20));

    for identity in identities.iter().take(20) {
        let points = match range {
            AnalyticsRange::Today => graph_today(connection, column, identity, &period)?,
            AnalyticsRange::SevenDays | AnalyticsRange::ThirtyDays => {
                graph_daily(connection, column, identity, &period)?
            }
            AnalyticsRange::All => graph_monthly(connection, column, identity, &period)?,
        };
        let by_bucket = points
            .into_iter()
            .map(|point| (point.bucket, point.count))
            .collect::<HashMap<_, _>>();
        series.push(UsageGraphSeries {
            identity: identity.clone(),
            points: buckets
                .iter()
                .map(|bucket| UsageGraphPoint {
                    bucket: bucket.clone(),
                    count: by_bucket.get(bucket).copied().unwrap_or(0),
                })
                .collect(),
        });
    }

    Ok(UsageGraphSnapshot {
        bucket_kind: bucket_kind.to_owned(),
        buckets,
        series,
    })
}

fn current_period(
    connection: &Connection,
    range: AnalyticsRange,
    now: NaiveDateTime,
) -> Result<Period, AnalyticsError> {
    let today = now.date();
    match range {
        AnalyticsRange::Today => Ok(Period {
            start_date: today,
            end_date: today,
            start_at: Some(today.and_time(NaiveTime::MIN)),
            end_at: Some(now),
            average_days: 1.0,
        }),
        AnalyticsRange::SevenDays => Ok(Period {
            start_date: today - Duration::days(6),
            end_date: today,
            start_at: None,
            end_at: None,
            average_days: 7.0,
        }),
        AnalyticsRange::ThirtyDays => Ok(Period {
            start_date: today - Duration::days(29),
            end_date: today,
            start_at: None,
            end_at: None,
            average_days: 30.0,
        }),
        AnalyticsRange::All => {
            let (first, last) = data_date_bounds(connection)?;
            let start = first.unwrap_or(today);
            let end = last.unwrap_or(today);
            let days = distinct_data_days(connection, start, end)?.max(1) as f64;
            Ok(Period {
                start_date: start,
                end_date: end,
                start_at: None,
                end_at: None,
                average_days: days,
            })
        }
    }
}

fn previous_period(range: AnalyticsRange, current: &Period, now: NaiveDateTime) -> Option<Period> {
    match range {
        AnalyticsRange::Today => {
            let previous_date = now.date() - Duration::days(1);
            Some(Period {
                start_date: previous_date,
                end_date: previous_date,
                start_at: Some(previous_date.and_time(NaiveTime::MIN)),
                end_at: Some(previous_date.and_time(now.time())),
                average_days: 1.0,
            })
        }
        AnalyticsRange::SevenDays => Some(Period {
            start_date: current.start_date - Duration::days(7),
            end_date: current.end_date - Duration::days(7),
            start_at: None,
            end_at: None,
            average_days: 7.0,
        }),
        AnalyticsRange::ThirtyDays => Some(Period {
            start_date: current.start_date - Duration::days(30),
            end_date: current.end_date - Duration::days(30),
            start_at: None,
            end_at: None,
            average_days: 30.0,
        }),
        AnalyticsRange::All => None,
    }
}

fn key_counts(
    connection: &Connection,
    period: &Period,
    application_filter: Option<&str>,
) -> Result<HashMap<String, i64>, AnalyticsError> {
    if let (Some(start_at), Some(end_at)) = (period.start_at, period.end_at) {
        let mut statement = connection.prepare(
            "SELECT shortcut, COUNT(*) FROM shortcut_events\n             WHERE occurred_at >= ?1 AND occurred_at <= ?2\n               AND (?3 IS NULL OR application = ?3)\n             GROUP BY shortcut",
        )?;
        let rows = statement.query_map(
            params![
                format_timestamp(start_at),
                format_timestamp(end_at),
                application_filter
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    } else {
        let mut statement = connection.prepare(
            "SELECT shortcut, SUM(count) FROM shortcut_daily\n             WHERE date >= ?1 AND date <= ?2\n               AND (?3 IS NULL OR application = ?3)\n             GROUP BY shortcut",
        )?;
        let rows = statement.query_map(
            params![
                date_text(period.start_date),
                date_text(period.end_date),
                application_filter
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    }
}

fn application_counts(
    connection: &Connection,
    period: &Period,
) -> Result<HashMap<String, i64>, AnalyticsError> {
    if let (Some(start_at), Some(end_at)) = (period.start_at, period.end_at) {
        let mut statement = connection.prepare(
            "SELECT application, COUNT(*) FROM shortcut_events\n             WHERE occurred_at >= ?1 AND occurred_at <= ?2 GROUP BY application",
        )?;
        let rows = statement.query_map(
            params![format_timestamp(start_at), format_timestamp(end_at)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    } else {
        let mut statement = connection.prepare(
            "SELECT application, SUM(count) FROM shortcut_daily\n             WHERE date >= ?1 AND date <= ?2 GROUP BY application",
        )?;
        let rows = statement.query_map(
            params![date_text(period.start_date), date_text(period.end_date)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    }
}

fn application_key_type_counts(
    connection: &Connection,
    period: &Period,
) -> Result<HashMap<String, i64>, AnalyticsError> {
    if let (Some(start_at), Some(end_at)) = (period.start_at, period.end_at) {
        let mut statement = connection.prepare(
            "SELECT application, COUNT(DISTINCT shortcut) FROM shortcut_events\n             WHERE occurred_at >= ?1 AND occurred_at <= ?2 GROUP BY application",
        )?;
        let rows = statement.query_map(
            params![format_timestamp(start_at), format_timestamp(end_at)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    } else {
        let mut statement = connection.prepare(
            "SELECT application, COUNT(DISTINCT shortcut) FROM shortcut_daily\n             WHERE date >= ?1 AND date <= ?2 GROUP BY application",
        )?;
        let rows = statement.query_map(
            params![date_text(period.start_date), date_text(period.end_date)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    }
}

fn application_active_seconds(
    connection: &Connection,
    period: &Period,
) -> Result<HashMap<String, i64>, AnalyticsError> {
    let mut statement = connection.prepare(
        "SELECT application, SUM(active_seconds) FROM application_usage_daily\n         WHERE date >= ?1 AND date <= ?2 GROUP BY application",
    )?;
    let rows = statement.query_map(
        params![date_text(period.start_date), date_text(period.end_date)],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

fn list_applications(connection: &Connection) -> Result<Vec<String>, AnalyticsError> {
    let mut statement = connection.prepare(
        "SELECT application FROM (\n           SELECT DISTINCT application FROM shortcut_daily\n           UNION SELECT DISTINCT application FROM shortcut_events\n           UNION SELECT DISTINCT application FROM application_usage_daily\n         ) WHERE application <> '' ORDER BY application COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn graph_today(
    connection: &Connection,
    column: &str,
    identity: &str,
    period: &Period,
) -> Result<Vec<UsageGraphPoint>, AnalyticsError> {
    let start = period.start_at.expect("today start");
    let end = period.end_at.expect("today end");
    let sql = format!(
        "SELECT substr(occurred_at, 12, 2) AS bucket, COUNT(*)\n         FROM shortcut_events WHERE occurred_at >= ?1 AND occurred_at <= ?2 AND {column} = ?3\n         GROUP BY bucket ORDER BY bucket"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![format_timestamp(start), format_timestamp(end), identity],
        |row| {
            Ok(UsageGraphPoint {
                bucket: row.get(0)?,
                count: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn graph_daily(
    connection: &Connection,
    column: &str,
    identity: &str,
    period: &Period,
) -> Result<Vec<UsageGraphPoint>, AnalyticsError> {
    let sql = format!(
        "SELECT date, SUM(count) FROM shortcut_daily\n         WHERE date >= ?1 AND date <= ?2 AND {column} = ?3\n         GROUP BY date ORDER BY date"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            date_text(period.start_date),
            date_text(period.end_date),
            identity
        ],
        |row| {
            Ok(UsageGraphPoint {
                bucket: row.get(0)?,
                count: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn graph_monthly(
    connection: &Connection,
    column: &str,
    identity: &str,
    period: &Period,
) -> Result<Vec<UsageGraphPoint>, AnalyticsError> {
    let sql = format!(
        "SELECT substr(date, 1, 7) AS bucket, SUM(count) FROM shortcut_daily\n         WHERE date >= ?1 AND date <= ?2 AND {column} = ?3\n         GROUP BY bucket ORDER BY bucket"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            date_text(period.start_date),
            date_text(period.end_date),
            identity
        ],
        |row| {
            Ok(UsageGraphPoint {
                bucket: row.get(0)?,
                count: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn buckets_for(
    range: AnalyticsRange,
    period: &Period,
    now: NaiveDateTime,
) -> (&'static str, Vec<String>) {
    match range {
        AnalyticsRange::Today => (
            "hour",
            (0..=now.time().hour())
                .map(|hour| format!("{hour:02}"))
                .collect(),
        ),
        AnalyticsRange::SevenDays | AnalyticsRange::ThirtyDays => {
            let mut buckets = Vec::new();
            let mut date = period.start_date;
            while date <= period.end_date {
                buckets.push(date_text(date));
                date += Duration::days(1);
            }
            ("day", buckets)
        }
        AnalyticsRange::All => {
            let mut buckets = Vec::new();
            let mut year = period.start_date.year();
            let mut month = period.start_date.month();
            loop {
                buckets.push(format!("{year:04}-{month:02}"));
                if year == period.end_date.year() && month == period.end_date.month() {
                    break;
                }
                month += 1;
                if month == 13 {
                    month = 1;
                    year += 1;
                }
            }
            ("month", buckets)
        }
    }
}

fn data_date_bounds(
    connection: &Connection,
) -> Result<(Option<NaiveDate>, Option<NaiveDate>), AnalyticsError> {
    let (first, last): (Option<String>, Option<String>) = connection.query_row(
        "SELECT MIN(date), MAX(date) FROM (\n           SELECT date FROM shortcut_daily\n           UNION ALL SELECT date FROM application_usage_daily\n         )",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((
        first.and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok()),
        last.and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok()),
    ))
}

fn distinct_data_days(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<i64, AnalyticsError> {
    Ok(connection.query_row(
        "SELECT COUNT(DISTINCT date) FROM shortcut_daily WHERE date >= ?1 AND date <= ?2",
        params![date_text(start), date_text(end)],
        |row| row.get(0),
    )?)
}

fn distinct_data_days_filtered(
    connection: &Connection,
    start: NaiveDate,
    end: NaiveDate,
    application_filter: Option<&str>,
) -> Result<i64, AnalyticsError> {
    Ok(connection.query_row(
        "SELECT COUNT(DISTINCT date) FROM shortcut_daily\n         WHERE date >= ?1 AND date <= ?2 AND (?3 IS NULL OR application = ?3)",
        params![date_text(start), date_text(end), application_filter],
        |row| row.get(0),
    )?)
}

fn change_rate(current: i64, previous: i64) -> Option<f64> {
    if previous == 0 {
        None
    } else {
        Some((current - previous) as f64 * 100.0 / previous as f64)
    }
}

fn percent(value: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        value as f64 * 100.0 / total as f64
    }
}

fn date_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn format_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shortcut_storage::migration::run_migrations;

    fn connection() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        run_migrations(&mut db).unwrap();
        db.execute("INSERT INTO shortcut_events(occurred_at,shortcut,application) VALUES ('2026-08-18 10:00:00.000','Ctrl+C','Code.exe')", []).unwrap();
        db.execute("INSERT INTO shortcut_events(occurred_at,shortcut,application) VALUES ('2026-08-17 10:00:00.000','Ctrl+C','Code.exe')", []).unwrap();
        db.execute("INSERT INTO shortcut_daily(date,shortcut,application,count) VALUES ('2026-08-18','Ctrl+C','Code.exe',4)", []).unwrap();
        db.execute("INSERT INTO shortcut_daily(date,shortcut,application,count) VALUES ('2026-08-17','Ctrl+C','Code.exe',2)", []).unwrap();
        db.execute("INSERT INTO application_usage_daily(date,application,active_seconds) VALUES ('2026-08-18','Code.exe',600)", []).unwrap();
        db
    }

    #[test]
    fn today_compares_same_clock_window() {
        let db = connection();
        let now = NaiveDate::from_ymd_opt(2026, 8, 18)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        let result = rankings(&db, AnalyticsRange::Today, None, now).unwrap();
        assert_eq!(result.key_rows[0].count, 1);
        assert_eq!(result.key_rows[0].comparison_rate, Some(0.0));
        assert_eq!(result.comparison_label.as_deref(), Some("前日比"));
    }

    #[test]
    fn all_range_average_uses_filtered_application_data_days() {
        let db = connection();
        db.execute("INSERT INTO shortcut_daily(date,shortcut,application,count) VALUES ('2026-08-16','Ctrl+V','msedge.exe',100)", []).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 8, 18)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        let result = rankings(&db, AnalyticsRange::All, Some("Code.exe"), now).unwrap();
        assert_eq!(result.key_rows[0].shortcut, "Ctrl+C");
        assert!((result.key_rows[0].average - 3.0).abs() < f64::EPSILON);
        assert!((result.key_rows[0].ratio - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn graph_today_returns_zero_filled_hours() {
        let db = connection();
        let now = NaiveDate::from_ymd_opt(2026, 8, 18)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        let result = usage_graph(
            &db,
            "keys",
            AnalyticsRange::Today,
            &["Ctrl+C".to_owned()],
            now,
        )
        .unwrap();
        assert_eq!(result.buckets.len(), 11);
        assert_eq!(result.series[0].points[10].count, 1);
    }
}
