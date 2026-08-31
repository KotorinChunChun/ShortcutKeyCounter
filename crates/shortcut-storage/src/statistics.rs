use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// 集計対象の日付範囲。開始日・終了日をともに含む。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Result<Self, StorageError> {
        if start > end {
            return Err(StorageError::InvalidDateRange);
        }
        Ok(Self { start, end })
    }

    fn start_text(&self) -> String {
        self.start.format("%Y-%m-%d").to_string()
    }

    fn end_text(&self) -> String {
        self.end.format("%Y-%m-%d").to_string()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RankedShortcut {
    pub shortcut: String,
    pub count: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RankedApplication {
    pub application: String,
    pub count: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShortcutRankingRow {
    pub shortcut: String,
    pub count: i64,
    pub application_count: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplicationStats {
    pub application: String,
    pub count: i64,
    pub unique_shortcuts: i64,
    pub most_used_shortcut: Option<String>,
    pub active_seconds: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DataBounds {
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DailyPoint {
    pub date: String,
    pub count: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HourlyPoint {
    pub hour: u8,
    pub count: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HourlyTrend {
    pub points: Vec<HourlyPoint>,
    pub first_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DashboardSummary {
    pub total_count: i64,
    pub unique_shortcuts: i64,
    pub unique_applications: i64,
    pub top_shortcut: Option<RankedShortcut>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShortcutDetail {
    pub shortcut: String,
    pub total_count: i64,
    pub applications: Vec<RankedApplication>,
    pub daily: Vec<DailyPoint>,
}

pub fn get_dashboard_summary(
    connection: &Connection,
    range: &DateRange,
) -> Result<DashboardSummary, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let total_count: i64 = connection.query_row(
        "SELECT COALESCE(SUM(count), 0) FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2",
        params![start, end],
        |row| row.get(0),
    )?;
    let unique_shortcuts: i64 = connection.query_row(
        "SELECT COUNT(DISTINCT shortcut) FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2",
        params![start, end],
        |row| row.get(0),
    )?;
    let unique_applications: i64 = connection.query_row(
        "SELECT COUNT(DISTINCT application) FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2",
        params![start, end],
        |row| row.get(0),
    )?;
    let top_shortcut = connection
        .query_row(
            "SELECT shortcut, SUM(count) AS total
             FROM shortcut_daily
             WHERE date >= ?1 AND date <= ?2
             GROUP BY shortcut
             ORDER BY total DESC, shortcut ASC
             LIMIT 1",
            params![start, end],
            |row| {
                Ok(RankedShortcut {
                    shortcut: row.get(0)?,
                    count: row.get(1)?,
                })
            },
        )
        .optional()?;

    Ok(DashboardSummary {
        total_count,
        unique_shortcuts,
        unique_applications,
        top_shortcut,
    })
}

pub fn get_shortcut_ranking(
    connection: &Connection,
    range: &DateRange,
    application: Option<&str>,
    limit: u32,
) -> Result<Vec<RankedShortcut>, StorageError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "SELECT shortcut, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR application = ?3)
         GROUP BY shortcut
         ORDER BY total DESC, shortcut ASC
         LIMIT ?4",
    )?;
    let rows = statement.query_map(params![start, end, application, i64::from(limit)], |row| {
        Ok(RankedShortcut {
            shortcut: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_application_ranking(
    connection: &Connection,
    range: &DateRange,
    limit: u32,
) -> Result<Vec<RankedApplication>, StorageError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "SELECT application, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2
         GROUP BY application
         ORDER BY total DESC, application ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(params![start, end, i64::from(limit)], |row| {
        Ok(RankedApplication {
            application: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_shortcut_ranking_extended(
    connection: &Connection,
    range: &DateRange,
    application: Option<&str>,
    limit: u32,
) -> Result<Vec<ShortcutRankingRow>, StorageError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "SELECT shortcut, SUM(count) AS total, COUNT(DISTINCT application) AS app_count
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR application = ?3)
         GROUP BY shortcut
         ORDER BY total DESC, shortcut ASC
         LIMIT ?4",
    )?;
    let rows = statement.query_map(params![start, end, application, i64::from(limit)], |row| {
        Ok(ShortcutRankingRow {
            shortcut: row.get(0)?,
            count: row.get(1)?,
            application_count: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_application_statistics(
    connection: &Connection,
    range: &DateRange,
) -> Result<Vec<ApplicationStats>, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "WITH apps AS (
            SELECT DISTINCT application FROM shortcut_daily WHERE date >= ?1 AND date <= ?2
            UNION
            SELECT DISTINCT application FROM application_usage_daily WHERE date >= ?1 AND date <= ?2
         )
         SELECT apps.application,
                COALESCE((SELECT SUM(d.count) FROM shortcut_daily d
                          WHERE d.application = apps.application AND d.date >= ?1 AND d.date <= ?2), 0) AS total,
                COALESCE((SELECT COUNT(DISTINCT d.shortcut) FROM shortcut_daily d
                          WHERE d.application = apps.application AND d.date >= ?1 AND d.date <= ?2), 0) AS unique_shortcuts,
                (
                    SELECT d2.shortcut FROM shortcut_daily d2
                    WHERE d2.application = apps.application AND d2.date >= ?1 AND d2.date <= ?2
                    GROUP BY d2.shortcut ORDER BY SUM(d2.count) DESC, d2.shortcut ASC LIMIT 1
                ) AS most_used_shortcut,
                COALESCE((SELECT SUM(u.active_seconds) FROM application_usage_daily u
                          WHERE u.application = apps.application AND u.date >= ?1 AND u.date <= ?2), 0) AS active_seconds
         FROM apps
         ORDER BY total DESC, active_seconds DESC, apps.application ASC",
    )?;
    let rows = statement.query_map(params![start, end], |row| {
        Ok(ApplicationStats {
            application: row.get(0)?,
            count: row.get(1)?,
            unique_shortcuts: row.get(2)?,
            most_used_shortcut: row.get(3)?,
            active_seconds: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_shortcuts(
    connection: &Connection,
    range: &DateRange,
) -> Result<Vec<RankedShortcut>, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "SELECT shortcut, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2
         GROUP BY shortcut
         ORDER BY total DESC, shortcut ASC",
    )?;
    let rows = statement.query_map(params![start, end], |row| {
        Ok(RankedShortcut {
            shortcut: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_data_bounds(connection: &Connection) -> Result<DataBounds, StorageError> {
    connection
        .query_row(
            "SELECT MIN(date), MAX(date) FROM shortcut_daily",
            [],
            |row| {
                Ok(DataBounds {
                    first_date: row.get(0)?,
                    last_date: row.get(1)?,
                })
            },
        )
        .map_err(Into::into)
}

pub fn get_daily_trend(
    connection: &Connection,
    range: &DateRange,
    shortcut: Option<&str>,
    application: Option<&str>,
) -> Result<Vec<DailyPoint>, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let mut statement = connection.prepare(
        "SELECT date, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2
           AND (?3 IS NULL OR shortcut = ?3)
           AND (?4 IS NULL OR application = ?4)
         GROUP BY date
         ORDER BY date ASC",
    )?;
    let rows = statement.query_map(params![start, end, shortcut, application], |row| {
        Ok(DailyPoint {
            date: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 指定期間の時間別集計を00時から23時まで欠損なく返す。
///
/// `first_date`は時間別集計を保持している最初の日付で、日次集計しか
/// 存在しないv0.1.5以前の期間をUIが注記するために使用する。
pub fn get_hourly_trend(
    connection: &Connection,
    range: &DateRange,
) -> Result<HourlyTrend, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let mut counts = [0_i64; 24];
    let mut statement = connection.prepare(
        "SELECT hour, SUM(count) AS total
         FROM shortcut_hourly_summary
         WHERE date >= ?1 AND date <= ?2
         GROUP BY hour
         ORDER BY hour ASC",
    )?;
    let rows = statement.query_map(params![start, end], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (hour, count) = row?;
        if let Ok(index) = usize::try_from(hour) {
            if let Some(slot) = counts.get_mut(index) {
                *slot = count;
            }
        }
    }
    let first_date: Option<String> =
        connection.query_row("SELECT MIN(date) FROM shortcut_hourly_summary", [], |row| {
            row.get(0)
        })?;
    Ok(HourlyTrend {
        points: counts
            .into_iter()
            .enumerate()
            .map(|(hour, count)| HourlyPoint {
                hour: hour as u8,
                count,
            })
            .collect(),
        first_date,
    })
}

pub fn get_shortcut_detail(
    connection: &Connection,
    shortcut: &str,
    range: &DateRange,
) -> Result<ShortcutDetail, StorageError> {
    let start = range.start_text();
    let end = range.end_text();
    let total_count: i64 = connection.query_row(
        "SELECT COALESCE(SUM(count), 0) FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2 AND shortcut = ?3",
        params![start, end, shortcut],
        |row| row.get(0),
    )?;

    let mut application_statement = connection.prepare(
        "SELECT application, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2 AND shortcut = ?3
         GROUP BY application
         ORDER BY total DESC, application ASC",
    )?;
    let applications = application_statement
        .query_map(params![start, end, shortcut], |row| {
            Ok(RankedApplication {
                application: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut daily_statement = connection.prepare(
        "SELECT date, SUM(count) AS total
         FROM shortcut_daily
         WHERE date >= ?1 AND date <= ?2 AND shortcut = ?3
         GROUP BY date
         ORDER BY date ASC",
    )?;
    let daily = daily_statement
        .query_map(params![start, end, shortcut], |row| {
            Ok(DailyPoint {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ShortcutDetail {
        shortcut: shortcut.to_owned(),
        total_count,
        applications,
        daily,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use shortcut_core::model::RecordableEvent;
    use shortcut_storage_test_support::seed_events;

    mod shortcut_storage_test_support {
        use super::*;
        use crate::migration::run_migrations;
        use crate::record_event;
        use rusqlite::Connection;

        pub fn seed_events() -> Connection {
            let mut connection = Connection::open_in_memory().expect("in-memory database");
            run_migrations(&mut connection).expect("migration");
            let samples = [
                ("2026-08-15", "Ctrl+C", "Code.exe", 2),
                ("2026-08-15", "Ctrl+V", "Code.exe", 1),
                ("2026-08-15", "Alt+Tab", "msedge.exe", 3),
                ("2026-08-16", "Ctrl+C", "Code.exe", 1),
                ("2026-08-16", "Ctrl+C", "EXCEL.EXE", 2),
            ];
            for (date, shortcut, application, count) in samples {
                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date");
                for index in 0..count {
                    let event = RecordableEvent {
                        occurred_at: date.and_hms_milli_opt(9, index as u32, 0, 0).expect("time"),
                        shortcut: shortcut.to_owned(),
                        application: application.to_owned(),
                    };
                    record_event(&mut connection, &event).expect("record event");
                }
            }
            connection
        }
    }

    fn range() -> DateRange {
        DateRange::new(
            NaiveDate::from_ymd_opt(2026, 8, 15).expect("start"),
            NaiveDate::from_ymd_opt(2026, 8, 16).expect("end"),
        )
        .expect("range")
    }

    #[test]
    fn date_range_rejects_reverse_order() {
        assert!(matches!(
            DateRange::new(
                NaiveDate::from_ymd_opt(2026, 8, 16).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()
            ),
            Err(StorageError::InvalidDateRange)
        ));
    }

    #[test]
    fn fixture_queries_return_expected_summary_and_rankings() {
        let connection = seed_events();
        let summary = get_dashboard_summary(&connection, &range()).expect("summary");
        assert_eq!(summary.total_count, 9);
        assert_eq!(summary.unique_shortcuts, 3);
        assert_eq!(summary.unique_applications, 3);
        assert_eq!(
            summary.top_shortcut,
            Some(RankedShortcut {
                shortcut: "Ctrl+C".to_owned(),
                count: 5,
            })
        );

        assert_eq!(
            get_shortcut_ranking(&connection, &range(), None, 2).expect("shortcut ranking"),
            vec![
                RankedShortcut {
                    shortcut: "Ctrl+C".to_owned(),
                    count: 5,
                },
                RankedShortcut {
                    shortcut: "Alt+Tab".to_owned(),
                    count: 3,
                },
            ]
        );
        assert_eq!(
            get_shortcut_ranking(&connection, &range(), Some("Code.exe"), 10)
                .expect("filtered ranking"),
            vec![
                RankedShortcut {
                    shortcut: "Ctrl+C".to_owned(),
                    count: 3,
                },
                RankedShortcut {
                    shortcut: "Ctrl+V".to_owned(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            get_application_ranking(&connection, &range(), 10).expect("application ranking"),
            vec![
                RankedApplication {
                    application: "Code.exe".to_owned(),
                    count: 4,
                },
                RankedApplication {
                    application: "msedge.exe".to_owned(),
                    count: 3,
                },
                RankedApplication {
                    application: "EXCEL.EXE".to_owned(),
                    count: 2,
                },
            ]
        );
    }

    #[test]
    fn extended_queries_return_application_counts_catalog_stats_and_bounds() {
        let connection = seed_events();
        assert_eq!(
            get_shortcut_ranking_extended(&connection, &range(), None, 10).expect("extended"),
            vec![
                ShortcutRankingRow {
                    shortcut: "Ctrl+C".to_owned(),
                    count: 5,
                    application_count: 2
                },
                ShortcutRankingRow {
                    shortcut: "Alt+Tab".to_owned(),
                    count: 3,
                    application_count: 1
                },
                ShortcutRankingRow {
                    shortcut: "Ctrl+V".to_owned(),
                    count: 1,
                    application_count: 1
                },
            ]
        );
        assert_eq!(
            get_shortcut_ranking_extended(&connection, &range(), Some("Code.exe"), 10)
                .expect("filtered extended")[0]
                .application_count,
            1
        );
        assert!(
            get_shortcut_ranking_extended(&connection, &range(), None, 0)
                .expect("zero")
                .is_empty()
        );

        let apps = get_application_statistics(&connection, &range()).expect("application stats");
        assert_eq!(
            apps[0],
            ApplicationStats {
                application: "Code.exe".to_owned(),
                count: 4,
                unique_shortcuts: 2,
                most_used_shortcut: Some("Ctrl+C".to_owned()),
                active_seconds: 0,
            }
        );
        assert_eq!(apps[1].application, "msedge.exe");
        assert_eq!(apps[1].most_used_shortcut.as_deref(), Some("Alt+Tab"));

        let catalog = list_shortcuts(&connection, &range()).expect("catalog");
        assert_eq!(catalog[0].shortcut, "Ctrl+C");
        assert_eq!(catalog.len(), 3);

        assert_eq!(
            get_data_bounds(&connection).expect("bounds"),
            DataBounds {
                first_date: Some("2026-08-15".to_owned()),
                last_date: Some("2026-08-16".to_owned()),
            }
        );
    }

    #[test]
    fn data_bounds_and_application_stats_support_empty_database() {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        crate::migration::run_migrations(&mut connection).expect("migration");
        assert_eq!(
            get_data_bounds(&connection).expect("bounds"),
            DataBounds {
                first_date: None,
                last_date: None
            }
        );
        assert!(get_application_statistics(&connection, &range())
            .expect("stats")
            .is_empty());
        assert!(list_shortcuts(&connection, &range())
            .expect("catalog")
            .is_empty());
    }

    #[test]
    fn hourly_trend_returns_all_hours_and_retained_data_start() {
        let connection = seed_events();
        let trend = get_hourly_trend(&connection, &range()).expect("hourly trend");

        assert_eq!(trend.first_date, Some("2026-08-15".to_owned()));
        assert_eq!(trend.points.len(), 24);
        assert_eq!(
            trend.points.first(),
            Some(&HourlyPoint { hour: 0, count: 0 })
        );
        assert_eq!(trend.points[9], HourlyPoint { hour: 9, count: 9 });
        assert_eq!(
            trend.points.last(),
            Some(&HourlyPoint { hour: 23, count: 0 })
        );
        assert_eq!(trend.points.iter().map(|point| point.count).sum::<i64>(), 9);
    }

    #[test]
    fn fixture_queries_return_trend_and_detail() {
        let connection = seed_events();
        assert_eq!(
            get_daily_trend(&connection, &range(), Some("Ctrl+C"), None).expect("trend"),
            vec![
                DailyPoint {
                    date: "2026-08-15".to_owned(),
                    count: 2,
                },
                DailyPoint {
                    date: "2026-08-16".to_owned(),
                    count: 3,
                },
            ]
        );

        assert_eq!(
            get_shortcut_detail(&connection, "Ctrl+C", &range()).expect("detail"),
            ShortcutDetail {
                shortcut: "Ctrl+C".to_owned(),
                total_count: 5,
                applications: vec![
                    RankedApplication {
                        application: "Code.exe".to_owned(),
                        count: 3,
                    },
                    RankedApplication {
                        application: "EXCEL.EXE".to_owned(),
                        count: 2,
                    },
                ],
                daily: vec![
                    DailyPoint {
                        date: "2026-08-15".to_owned(),
                        count: 2,
                    },
                    DailyPoint {
                        date: "2026-08-16".to_owned(),
                        count: 3,
                    },
                ],
            }
        );
    }
}
