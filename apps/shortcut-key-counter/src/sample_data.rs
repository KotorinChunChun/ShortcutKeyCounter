use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SampleDataError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("sample source has no applications or shortcuts")]
    EmptySource,
}

#[derive(Clone, Debug, Serialize)]
pub struct SampleSourceStatus {
    pub application_count: u64,
    pub shortcut_count: u64,
    pub can_generate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SampleGenerationResult {
    pub events_generated: u64,
    pub usage_rows_generated: u64,
    pub period_start: String,
    pub period_end: String,
    pub realtime_log_rows: u64,
}

pub fn source_status(production: &Connection) -> Result<SampleSourceStatus, SampleDataError> {
    let applications = distinct_values(production, "application")?;
    let shortcut_count = distinct_values(production, "shortcut")?
        .into_iter()
        .filter(|shortcut| is_recordable_sample_shortcut(shortcut))
        .count() as u64;
    Ok(SampleSourceStatus {
        application_count: applications.len() as u64,
        shortcut_count,
        can_generate: !applications.is_empty() && shortcut_count > 0,
    })
}

pub fn generate_sample_data(
    production: &Connection,
    target: &mut Connection,
    event_count: usize,
) -> Result<SampleGenerationResult, SampleDataError> {
    let applications = distinct_values(production, "application")?;
    let shortcuts = distinct_values(production, "shortcut")?
        .into_iter()
        .filter(|shortcut| is_recordable_sample_shortcut(shortcut))
        .collect::<Vec<_>>();
    if applications.is_empty() || shortcuts.is_empty() {
        return Err(SampleDataError::EmptySource);
    }

    let yesterday = Local::now().date_naive() - Duration::days(1);
    let start = yesterday - Duration::days(59);
    let tx = target.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut rng = Lcg::new(Local::now().timestamp_millis() as u64 ^ 0x5A17_C0DE);

    // Human-like model:
    // 1) decide a correlated daily activity level,
    // 2) distribute it to applications with weekday/weekend characteristics,
    // 3) distribute each application's events to shortcuts using a long-tail + learning curve.
    let day_weights = build_day_weights(start, &mut rng);
    let guaranteed_previous_day_events = event_count.min(200);
    let daily_event_counts =
        allocate_day_counts(event_count, &day_weights, guaranteed_previous_day_events);
    let app_base_weights = rank_weights(applications.len(), 1.16);
    let shortcut_base_weights = rank_weights(shortcuts.len(), 1.42);
    let mut daily_app_counts = vec![vec![0usize; applications.len()]; 60];

    for (day_index, daily_count) in daily_event_counts.into_iter().enumerate() {
        let date = start + Duration::days(day_index as i64);
        for _ in 0..daily_count {
            let app_weights = applications
                .iter()
                .enumerate()
                .map(|(rank, application)| {
                    app_base_weights[rank]
                        * application_day_factor(application, rank, date, day_index)
                        * (0.94 + rng.unit() * 0.12)
                })
                .collect::<Vec<_>>();
            let app_index = weighted_index(&app_weights, &mut rng);
            let application = &applications[app_index];

            let shortcut_weights = shortcuts
                .iter()
                .enumerate()
                .map(|(rank, shortcut)| {
                    shortcut_base_weights[rank]
                        * shortcut_learning_factor(rank, shortcuts.len(), day_index, shortcut)
                        * shortcut_application_affinity(application, shortcut)
                        * shortcut_day_factor(rank, day_index)
                        * (0.92 + rng.unit() * 0.16)
                })
                .collect::<Vec<_>>();
            let shortcut_index = weighted_index(&shortcut_weights, &mut rng);
            let shortcut = &shortcuts[shortcut_index];

            let time = sample_event_time(date, &mut rng);
            let occurred = NaiveDateTime::new(date, time);
            tx.execute(
                "INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES (?1, ?2, ?3)",
                params![occurred.format("%Y-%m-%d %H:%M:%S%.3f").to_string(), shortcut, application],
            )?;
            tx.execute(
                "INSERT INTO shortcut_daily(date, shortcut, application, count) VALUES (?1, ?2, ?3, 1)\n                 ON CONFLICT(date, shortcut, application) DO UPDATE SET count = count + 1",
                params![date.format("%Y-%m-%d").to_string(), shortcut, application],
            )?;
            tx.execute(
                "INSERT INTO shortcut_hourly_summary(date, hour, count) VALUES (?1, ?2, 1)\n                 ON CONFLICT(date, hour) DO UPDATE SET count = count + 1",
                params![date.format("%Y-%m-%d").to_string(), i64::from(occurred.hour())],
            )?;
            daily_app_counts[day_index][app_index] += 1;
        }
    }

    // Application usage follows the same daily/app profile as shortcut activity instead of
    // being generated independently. This keeps usage time and shortcut counts plausibly related.
    let mut usage_rows = 0u64;
    for (day_index, app_counts_for_day) in daily_app_counts.iter().enumerate() {
        let date = start + Duration::days(day_index as i64);
        let weekday = date.weekday();
        let weekend = matches!(weekday, Weekday::Sat | Weekday::Sun);
        let progress = day_index as f64 / 59.0;
        let base_hours = if weekend {
            2.2 + 2.4 * progress
        } else {
            6.0 + 2.3 * progress
        };
        let jitter = 0.88 + rng.unit() * 0.24;
        let daily_budget =
            ((base_hours * jitter * 3600.0).round() as i64).clamp(45 * 60, 14 * 3600);

        let usage_weights = applications
            .iter()
            .enumerate()
            .map(|(rank, application)| {
                let event_signal = (app_counts_for_day[rank] as f64 + 1.0).sqrt();
                event_signal
                    * app_base_weights[rank].sqrt()
                    * application_usage_factor(application, date)
                    * (0.94 + rng.unit() * 0.12)
            })
            .collect::<Vec<_>>();
        let seconds_by_app = allocate_seconds(daily_budget, &usage_weights);
        let mut inserted_for_day = false;
        for (app_index, seconds) in seconds_by_app.into_iter().enumerate() {
            if seconds < 60 {
                continue;
            }
            tx.execute(
                "INSERT INTO application_usage_daily(date, application, active_seconds) VALUES (?1, ?2, ?3)\n                 ON CONFLICT(date, application) DO UPDATE SET active_seconds = active_seconds + excluded.active_seconds",
                params![date.format("%Y-%m-%d").to_string(), &applications[app_index], seconds],
            )?;
            inserted_for_day = true;
            usage_rows += 1;
        }
        // Keep every calendar day represented even for very small test populations.
        if !inserted_for_day {
            tx.execute(
                "INSERT INTO application_usage_daily(date, application, active_seconds) VALUES (?1, ?2, 60)\n                 ON CONFLICT(date, application) DO UPDATE SET active_seconds = active_seconds + 60",
                params![date.format("%Y-%m-%d").to_string(), &applications[0]],
            )?;
            usage_rows += 1;
        }
    }

    tx.commit()?;
    Ok(SampleGenerationResult {
        events_generated: event_count as u64,
        usage_rows_generated: usage_rows,
        period_start: start.format("%Y-%m-%d").to_string(),
        period_end: yesterday.format("%Y-%m-%d").to_string(),
        realtime_log_rows: 0,
    })
}

fn distinct_values(connection: &Connection, column: &str) -> Result<Vec<String>, rusqlite::Error> {
    // Productionでよく使われているidentityを先頭にし、生成側でlong-tail分布を作る。
    let sql = format!(
        "SELECT {column}, SUM(weight) AS total FROM (\n           SELECT {column}, 1 AS weight FROM shortcut_events\n           UNION ALL SELECT {column}, count AS weight FROM shortcut_daily\n         ) WHERE {column} <> '' GROUP BY {column} ORDER BY total DESC, {column}"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn is_recordable_sample_shortcut(shortcut: &str) -> bool {
    let mut has_modifier = false;
    let mut has_primary = false;
    for part in shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if matches!(part, "Ctrl" | "Alt" | "Shift" | "Win") {
            has_modifier = true;
        } else {
            has_primary = true;
        }
    }
    has_modifier && has_primary
}

fn build_day_weights(start: NaiveDate, rng: &mut Lcg) -> Vec<f64> {
    let mut result = Vec::with_capacity(60);
    let mut smoothed_noise = 1.0;
    for day_index in 0..60 {
        let date = start + Duration::days(day_index as i64);
        let progress = day_index as f64 / 59.0;
        // Shortcut use rises over the trial period as the user memorises more keys.
        let learning_trend = 0.52 + 0.78 * progress.powf(1.15);
        let weekday_factor = match date.weekday() {
            Weekday::Mon => 0.98,
            Weekday::Tue => 1.05,
            Weekday::Wed => 1.08,
            Weekday::Thu => 1.04,
            Weekday::Fri => 0.92,
            Weekday::Sat => 0.46,
            Weekday::Sun => 0.38,
        };
        let target_noise = 0.84 + rng.unit() * 0.32;
        smoothed_noise = smoothed_noise * 0.72 + target_noise * 0.28;
        // A few multi-day work bursts create natural local peaks instead of isolated spikes.
        let burst = 1.0
            + triangular_peak(day_index, 16, 4, 0.18)
            + triangular_peak(day_index, 37, 5, 0.28)
            + triangular_peak(day_index, 52, 4, 0.22);
        result.push((learning_trend * weekday_factor * smoothed_noise * burst).max(0.05));
    }
    result
}

fn triangular_peak(day: usize, center: usize, half_width: usize, amplitude: f64) -> f64 {
    let distance = day.abs_diff(center);
    if distance > half_width {
        0.0
    } else {
        amplitude * (1.0 - distance as f64 / (half_width + 1) as f64)
    }
}

fn allocate_day_counts(total: usize, weights: &[f64], minimum_last_day: usize) -> Vec<usize> {
    if weights.is_empty() || total == 0 {
        return vec![0; weights.len()];
    }
    let minimum_each = usize::from(total >= weights.len());
    let reserved = minimum_each * weights.len();
    let distributable = total.saturating_sub(reserved);
    let weight_sum = weights.iter().sum::<f64>().max(f64::EPSILON);
    let mut counts = vec![minimum_each; weights.len()];
    let mut fractions = Vec::with_capacity(weights.len());
    let mut used = reserved;
    for (index, weight) in weights.iter().enumerate() {
        let exact = distributable as f64 * *weight / weight_sum;
        let whole = exact.floor() as usize;
        counts[index] += whole;
        used += whole;
        fractions.push((index, exact - whole as f64));
    }
    fractions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (index, _) in fractions.into_iter().take(total.saturating_sub(used)) {
        counts[index] += 1;
    }

    let last = counts.len() - 1;
    let target_last = minimum_last_day.min(total.saturating_sub(minimum_each * (counts.len() - 1)));
    if counts[last] < target_last {
        let mut deficit = target_last - counts[last];
        let mut donors = (0..last).collect::<Vec<_>>();
        donors.sort_by_key(|index| std::cmp::Reverse(counts[*index]));
        for donor in donors {
            if deficit == 0 {
                break;
            }
            let available = counts[donor].saturating_sub(minimum_each);
            let moved = available.min(deficit);
            counts[donor] -= moved;
            counts[last] += moved;
            deficit -= moved;
        }
    }
    counts
}

fn rank_weights(len: usize, exponent: f64) -> Vec<f64> {
    (0..len)
        .map(|rank| 1.0 / ((rank + 1) as f64).powf(exponent))
        .collect()
}

fn weighted_index(weights: &[f64], rng: &mut Lcg) -> usize {
    if weights.len() <= 1 {
        return 0;
    }
    let total = weights
        .iter()
        .copied()
        .filter(|value| *value > 0.0)
        .sum::<f64>();
    if total <= f64::EPSILON {
        return rng.range(weights.len() as u64) as usize;
    }
    let mut cursor = rng.unit() * total;
    for (index, weight) in weights.iter().enumerate() {
        if *weight <= 0.0 {
            continue;
        }
        if cursor <= *weight {
            return index;
        }
        cursor -= *weight;
    }
    weights.len() - 1
}

fn application_day_factor(
    application: &str,
    rank: usize,
    date: NaiveDate,
    day_index: usize,
) -> f64 {
    let lower = application.to_ascii_lowercase();
    let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
    let weekend_factor = if contains_any(
        &lower,
        &[
            "discord", "browser", "chrome", "edge", "msedge", "firefox", "chatgpt", "explorer",
        ],
    ) {
        0.72
    } else if contains_any(
        &lower,
        &[
            "teams",
            "excel",
            "outlook",
            "word",
            "powerpnt",
            "code",
            "terminal",
            "powershell",
        ],
    ) {
        0.24
    } else {
        0.48
    };
    let schedule = if weekend { weekend_factor } else { 1.0 };
    let phase = rank as f64 * 0.83 + stable_hash(application) as f64 % 17.0 / 10.0;
    let wave = 1.0 + 0.16 * (day_index as f64 * 0.39 + phase).sin();
    let progress = day_index as f64 / 59.0;
    let familiarity = if rank < 5 {
        0.90 + 0.18 * progress
    } else {
        0.78 + 0.34 * progress
    };
    (schedule * wave * familiarity).max(0.05)
}

fn application_usage_factor(application: &str, date: NaiveDate) -> f64 {
    let lower = application.to_ascii_lowercase();
    if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        return 1.0;
    }
    if contains_any(
        &lower,
        &[
            "discord", "chrome", "edge", "msedge", "firefox", "chatgpt", "explorer",
        ],
    ) {
        0.78
    } else if contains_any(
        &lower,
        &[
            "teams",
            "excel",
            "outlook",
            "word",
            "powerpnt",
            "code",
            "terminal",
            "powershell",
        ],
    ) {
        0.32
    } else {
        0.52
    }
}

fn shortcut_learning_factor(
    rank: usize,
    population: usize,
    day_index: usize,
    shortcut: &str,
) -> f64 {
    let progress = day_index as f64 / 59.0;
    if rank < 2 {
        return 0.90 + 0.55 * progress;
    }
    if rank < 5 {
        return 0.70 + 0.75 * progress;
    }
    let spread = population.max(8);
    let rank_progress = rank as f64 / spread as f64;
    let hash_offset = (stable_hash(shortcut) % 7) as usize;
    let start_day = ((rank_progress * 42.0).round() as usize + hash_offset).min(50);
    if day_index < start_day {
        return 0.015;
    }
    let learned_progress = (day_index - start_day) as f64 / (59 - start_day).max(1) as f64;
    0.20 + 1.25 * learned_progress.powf(0.85)
}

fn shortcut_day_factor(rank: usize, day_index: usize) -> f64 {
    let phase = rank as f64 * 0.61;
    (1.0 + 0.11 * (day_index as f64 * 0.47 + phase).sin()).max(0.70)
}

fn shortcut_application_affinity(application: &str, shortcut: &str) -> f64 {
    let app = application.to_ascii_lowercase();
    let key = shortcut.to_ascii_lowercase();
    let mut affinity = 0.72 + (stable_hash(&format!("{app}|{key}")) % 70) as f64 / 100.0;

    if contains_any(&app, &["code", "terminal", "powershell"])
        && contains_any(
            &key,
            &[
                "ctrl+s",
                "ctrl+c",
                "ctrl+v",
                "ctrl+f",
                "ctrl+shift",
                "alt+tab",
            ],
        )
    {
        affinity *= 1.65;
    }
    if contains_any(&app, &["chrome", "edge", "msedge", "firefox"])
        && contains_any(
            &key,
            &[
                "ctrl+l",
                "ctrl+t",
                "ctrl+w",
                "ctrl+f",
                "ctrl+c",
                "ctrl+v",
                "alt+left",
                "alt+right",
            ],
        )
    {
        affinity *= 1.70;
    }
    if app.contains("excel")
        && contains_any(
            &key,
            &[
                "ctrl+c",
                "ctrl+v",
                "ctrl+s",
                "ctrl+a",
                "ctrl+z",
                "ctrl+f",
                "ctrl+space",
                "shift+space",
            ],
        )
    {
        affinity *= 1.85;
    }
    if contains_any(&app, &["teams", "discord"])
        && contains_any(
            &key,
            &["ctrl+enter", "ctrl+shift", "alt+tab", "ctrl+c", "ctrl+v"],
        )
    {
        affinity *= 1.45;
    }
    if app.contains("explorer")
        && contains_any(
            &key,
            &["ctrl+c", "ctrl+v", "ctrl+shift", "alt+left", "alt+right"],
        )
    {
        affinity *= 1.55;
    }
    if app.contains("chatgpt") && contains_any(&key, &["ctrl+c", "ctrl+v", "ctrl+enter", "ctrl+a"])
    {
        affinity *= 1.55;
    }
    affinity.clamp(0.25, 3.0)
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn stable_hash(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(1469598103934665603u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
        })
}

fn sample_event_time(date: NaiveDate, rng: &mut Lcg) -> NaiveTime {
    let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
    let (start_hour, span_hours) = if weekend {
        (9u32, 14u32)
    } else if rng.range(100) < 82 {
        (8u32, 11u32)
    } else {
        (18u32, 5u32)
    };
    let seconds = start_hour * 3600 + rng.range((span_hours * 3600) as u64) as u32;
    let millis = rng.range(1000) as u32;
    NaiveTime::from_num_seconds_from_midnight_opt(seconds.min(86_399), millis * 1_000_000)
        .unwrap_or(NaiveTime::MIN)
}

fn allocate_seconds(total: i64, weights: &[f64]) -> Vec<i64> {
    if weights.is_empty() || total <= 0 {
        return vec![0; weights.len()];
    }
    let sum = weights
        .iter()
        .copied()
        .filter(|weight| *weight > 0.0)
        .sum::<f64>()
        .max(f64::EPSILON);
    let mut result = vec![0i64; weights.len()];
    let mut used = 0i64;
    let mut fractions = Vec::with_capacity(weights.len());
    for (index, weight) in weights.iter().enumerate() {
        let exact = total as f64 * weight.max(0.0) / sum;
        let whole = exact.floor() as i64;
        result[index] = whole;
        used += whole;
        fractions.push((index, exact - whole as f64));
    }
    fractions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (index, _) in fractions.into_iter().take((total - used).max(0) as usize) {
        result[index] += 1;
    }
    result
}

pub fn previous_day_realtime_samples(
    connection: &Connection,
    day: NaiveDate,
    limit: usize,
) -> Result<Vec<(NaiveDateTime, String, String)>, SampleDataError> {
    let start = format!("{} 00:00:00.000", day.format("%Y-%m-%d"));
    let end = format!("{} 23:59:59.999", day.format("%Y-%m-%d"));
    let mut statement = connection.prepare(
        "SELECT occurred_at, shortcut, application FROM shortcut_events
         WHERE occurred_at >= ?1 AND occurred_at <= ?2
         ORDER BY occurred_at DESC LIMIT ?3",
    )?;
    let rows = statement.query_map(params![start, end, limit.min(500) as i64], |row| {
        let raw: String = row.get(0)?;
        let occurred_at = NaiveDateTime::parse_from_str(&raw, "%Y-%m-%d %H:%M:%S%.3f")
            .or_else(|_| NaiveDateTime::parse_from_str(&raw, "%Y-%m-%d %H:%M:%S"))
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        Ok((occurred_at, row.get(1)?, row.get(2)?))
    })?;
    let mut values = rows.collect::<Result<Vec<_>, _>>()?;
    values.reverse();
    Ok(values)
}

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn range(&mut self, upper: u64) -> u64 {
        if upper == 0 {
            0
        } else {
            self.next() % upper
        }
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shortcut_storage::migration::run_migrations;

    fn database() -> Connection {
        let mut db = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut db).expect("migration");
        db
    }

    #[test]
    fn historical_unmodified_shortcuts_are_excluded_from_sample_population() {
        let production = database();
        production.execute(
            "INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES ('2026-08-01 09:00:00.000', 'A', 'Code.exe')",
            [],
        ).unwrap();
        production.execute(
            "INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES ('2026-08-01 09:01:00.000', 'Ctrl', 'Code.exe')",
            [],
        ).unwrap();
        let status = source_status(&production).unwrap();
        assert_eq!(status.application_count, 1);
        assert_eq!(status.shortcut_count, 0);
        assert!(!status.can_generate);
        let mut target = database();
        assert!(matches!(
            generate_sample_data(&production, &mut target, 10),
            Err(SampleDataError::EmptySource)
        ));
    }

    #[test]
    fn generated_sample_spans_exactly_sixty_calendar_days_ending_yesterday() {
        let production = database();
        production.execute(
            "INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES ('2026-08-01 10:00:00.000', 'Ctrl+C', 'Code.exe')",
            [],
        ).unwrap();
        production.execute(
            "INSERT INTO shortcut_daily(date, shortcut, application, count) VALUES ('2026-08-01', 'Ctrl+C', 'Code.exe', 1)",
            [],
        ).unwrap();
        let mut target = database();

        let result = generate_sample_data(&production, &mut target, 1_000).unwrap();
        let start = NaiveDate::parse_from_str(&result.period_start, "%Y-%m-%d").unwrap();
        let end = NaiveDate::parse_from_str(&result.period_end, "%Y-%m-%d").unwrap();
        assert_eq!(end.signed_duration_since(start).num_days(), 59);
        assert_eq!(end, Local::now().date_naive() - Duration::days(1));
        assert_eq!(result.events_generated, 1_000);

        let hourly_total: i64 = target
            .query_row(
                "SELECT COALESCE(SUM(count), 0) FROM shortcut_hourly_summary",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hourly_total, 1_000);

        let usage_days: i64 = target
            .query_row(
                "SELECT COUNT(DISTINCT date) FROM application_usage_daily",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(usage_days, 60);
        let max_daily_seconds: i64 = target
            .query_row(
                "SELECT COALESCE(MAX(total), 0) FROM (SELECT date, SUM(active_seconds) AS total FROM application_usage_daily GROUP BY date)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(max_daily_seconds <= 24 * 60 * 60);
    }

    #[test]
    fn daily_allocation_preserves_total_and_previous_day_minimum() {
        let counts = allocate_day_counts(500, &vec![1.0; 60], 200);
        assert_eq!(counts.iter().sum::<usize>(), 500);
        assert_eq!(counts.len(), 60);
        assert!(counts.iter().all(|count| *count >= 1));
        assert_eq!(counts[59], 200);
    }

    #[test]
    fn activity_model_trends_up_without_alternating_zero_days() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 19).unwrap();
        let mut rng = Lcg::new(42);
        let weights = build_day_weights(start, &mut rng);
        let counts = allocate_day_counts(10_000, &weights, 200);
        let early = counts[..10].iter().sum::<usize>() as f64 / 10.0;
        let late = counts[50..].iter().sum::<usize>() as f64 / 10.0;
        assert!(late > early * 1.4);
        assert!(counts.iter().all(|count| *count > 0));
    }

    #[test]
    fn previous_day_realtime_rows_are_read_from_persisted_sample_events() {
        let production = database();
        production.execute(
            "INSERT INTO shortcut_events(occurred_at, shortcut, application) VALUES ('2026-08-01 10:00:00.000', 'Ctrl+C', 'Code.exe')",
            [],
        ).unwrap();
        let mut target = database();
        generate_sample_data(&production, &mut target, 500).unwrap();
        let day = Local::now().date_naive() - Duration::days(1);
        let rows = previous_day_realtime_samples(&target, day, 200).unwrap();
        assert_eq!(rows.len(), 200);
        assert!(rows
            .iter()
            .all(|(occurred_at, _, _)| occurred_at.date() == day));
    }
}
