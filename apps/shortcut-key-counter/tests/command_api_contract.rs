//! command_apiの利用者契約テスト（v0.2.0 D03-T01）。
//!
//! Tauri commandが呼ぶバックエンドAPIを、製品同梱のmigrationを適用した
//! in-memory SQLiteへ実行し、UIが依存する戻り値・期間境界・検証・エラーを固定する。
//! 自動起動、Hook、UACなどWindows APIへ触れる経路は対象外とする。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use shortcut_core::key_mapping::KeyMapping;
use shortcut_core::model::RecordableEvent;
use shortcut_key_counter::capture_worker::shared_excluded_apps;
use shortcut_key_counter::command_api::{self, ExclusionCommandError};
use shortcut_key_counter::key_policy::{KeyAction, KeyPolicyConfig};
use shortcut_key_counter::maintenance::MaintenanceError;
use shortcut_key_counter::mapping_edit_session::{
    save_all_atomic, MappingEditError, MappingEditSession, MappingEditValues,
};
use shortcut_key_counter::overlay::config::{OverlayKeyLabels, OverlayPaths};
use shortcut_key_counter::settings::SettingsError;
use shortcut_storage::migration::run_migrations;
use shortcut_storage::record_event;
use shortcut_storage::repositories::excluded_app_repository::seed_default_excluded_apps;
use shortcut_storage::StorageError;

// ---------------------------------------------------------------------------
// fixture

fn empty_database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("in-memory database");
    run_migrations(&mut connection).expect("migration");
    seed_default_excluded_apps(&connection).expect("default excluded apps");
    connection
}

/// 期間の内側と、前後1秒外側にイベントを置いたDB。
///
/// | 日付 | 時刻 | ショートカット | アプリ |
/// |---|---|---|---|
/// | 08-14 | 23:59:59 | Ctrl+S | Code.exe（期間外） |
/// | 08-15 | 00:00:00 | Ctrl+C | Code.exe |
/// | 08-15 | 09:10:00 | Ctrl+C | Code.exe |
/// | 08-15 | 09:20:00 | Ctrl+V | EXCEL.EXE |
/// | 08-16 | 12:00:00 | Ctrl+V | EXCEL.EXE |
/// | 08-16 | 23:59:59 | Ctrl+C | EXCEL.EXE |
/// | 08-17 | 00:00:00 | Alt+Tab | msedge.exe（期間外） |
fn populated_database() -> Connection {
    let mut connection = empty_database();
    for (occurred_at, shortcut, application) in [
        ("2026-08-14 23:59:59", "Ctrl+S", "Code.exe"),
        ("2026-08-15 00:00:00", "Ctrl+C", "Code.exe"),
        ("2026-08-15 09:10:00", "Ctrl+C", "Code.exe"),
        ("2026-08-15 09:20:00", "Ctrl+V", "EXCEL.EXE"),
        ("2026-08-16 12:00:00", "Ctrl+V", "EXCEL.EXE"),
        ("2026-08-16 23:59:59", "Ctrl+C", "EXCEL.EXE"),
        ("2026-08-17 00:00:00", "Alt+Tab", "msedge.exe"),
    ] {
        let event = RecordableEvent {
            occurred_at: NaiveDateTime::parse_from_str(occurred_at, "%Y-%m-%d %H:%M:%S")
                .expect("timestamp"),
            shortcut: shortcut.to_owned(),
            application: application.to_owned(),
        };
        record_event(&mut connection, &event).expect("record event");
    }
    connection
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).expect("date")
}

/// テストごとに作り、Drop時に削除する一時ディレクトリ。
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "shortcut-key-counter-contract-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read_csv_lines(path: &Path) -> Vec<String> {
    let bytes = fs::read(path).expect("CSV bytes");
    assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "UTF-8 BOM");
    String::from_utf8(bytes[3..].to_vec())
        .expect("UTF-8")
        .split("\r\n")
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// 統計: ダッシュボード・ランキング・推移

#[test]
fn empty_database_returns_zero_values_instead_of_errors() {
    let connection = empty_database();
    let (start, end) = (date(15), date(16));

    let summary = command_api::dashboard_summary(&connection, start, end).expect("summary");
    assert_eq!(summary.total_count, 0);
    assert_eq!(summary.unique_shortcuts, 0);
    assert_eq!(summary.unique_applications, 0);
    assert_eq!(summary.top_shortcut, None);

    assert!(
        command_api::shortcut_ranking(&connection, start, end, None, 10)
            .expect("ranking")
            .is_empty()
    );
    assert!(
        command_api::shortcut_ranking_extended(&connection, start, end, None, 10)
            .expect("extended ranking")
            .is_empty()
    );
    assert!(
        command_api::application_ranking(&connection, start, end, 10)
            .expect("application ranking")
            .is_empty()
    );
    assert!(command_api::application_statistics(&connection, start, end)
        .expect("application statistics")
        .is_empty());
    assert!(command_api::shortcut_catalog(&connection, start, end)
        .expect("catalog")
        .is_empty());
    assert!(
        command_api::daily_trend(&connection, start, end, None, None)
            .expect("daily trend")
            .is_empty()
    );

    let bounds = command_api::data_bounds(&connection).expect("bounds");
    assert_eq!(bounds.first_date, None);
    assert_eq!(bounds.last_date, None);

    let hourly = command_api::hourly_trend(&connection, start, end).expect("hourly");
    assert_eq!(hourly.points.len(), 24);
    assert!(hourly.points.iter().all(|point| point.count == 0));
    assert_eq!(hourly.first_date, None);

    let detail = command_api::shortcut_detail(&connection, "Ctrl+C", start, end).expect("detail");
    assert_eq!(detail.total_count, 0);
    assert!(detail.applications.is_empty());
    assert!(detail.daily.is_empty());
}

#[test]
fn period_includes_both_end_dates_and_excludes_adjacent_days() {
    let connection = populated_database();

    let summary = command_api::dashboard_summary(&connection, date(15), date(16)).expect("summary");
    assert_eq!(summary.total_count, 5);
    assert_eq!(summary.unique_shortcuts, 2);
    assert_eq!(summary.unique_applications, 2);
    let top = summary.top_shortcut.expect("top shortcut");
    assert_eq!((top.shortcut.as_str(), top.count), ("Ctrl+C", 3));

    // 1日だけの期間も開始日=終了日として有効。
    let single_day =
        command_api::dashboard_summary(&connection, date(14), date(14)).expect("single day");
    assert_eq!(single_day.total_count, 1);

    let bounds = command_api::data_bounds(&connection).expect("bounds");
    assert_eq!(bounds.first_date.as_deref(), Some("2026-08-14"));
    assert_eq!(bounds.last_date.as_deref(), Some("2026-08-17"));
}

#[test]
fn reversed_period_is_rejected_by_every_period_query() {
    let connection = populated_database();
    let (start, end) = (date(16), date(15));
    let rejected =
        |result: Result<(), StorageError>| matches!(result, Err(StorageError::InvalidDateRange));

    assert!(rejected(
        command_api::dashboard_summary(&connection, start, end).map(drop)
    ));
    assert!(rejected(
        command_api::shortcut_ranking(&connection, start, end, None, 10).map(drop)
    ));
    assert!(rejected(
        command_api::shortcut_ranking_extended(&connection, start, end, None, 10).map(drop)
    ));
    assert!(rejected(
        command_api::application_ranking(&connection, start, end, 10).map(drop)
    ));
    assert!(rejected(
        command_api::application_statistics(&connection, start, end).map(drop)
    ));
    assert!(rejected(
        command_api::shortcut_catalog(&connection, start, end).map(drop)
    ));
    assert!(rejected(
        command_api::daily_trend(&connection, start, end, None, None).map(drop)
    ));
    assert!(rejected(
        command_api::hourly_trend(&connection, start, end).map(drop)
    ));
    assert!(rejected(
        command_api::shortcut_detail(&connection, "Ctrl+C", start, end).map(drop)
    ));
}

#[test]
fn rankings_sort_by_count_then_name_and_respect_filters_and_limit() {
    let connection = populated_database();
    let ranking = |start, end, application, limit| {
        command_api::shortcut_ranking(&connection, start, end, application, limit)
            .expect("ranking")
            .into_iter()
            .map(|row| (row.shortcut, row.count))
            .collect::<Vec<_>>()
    };
    let pairs = |rows: &[(&str, i64)]| {
        rows.iter()
            .map(|(name, count)| ((*name).to_owned(), *count))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        ranking(date(15), date(16), None, 10),
        pairs(&[("Ctrl+C", 3), ("Ctrl+V", 2)])
    );
    assert_eq!(
        ranking(date(15), date(16), None, 1),
        pairs(&[("Ctrl+C", 3)])
    );
    assert!(ranking(date(15), date(16), None, 0).is_empty());
    // 同数は名前の昇順。
    assert_eq!(
        ranking(date(16), date(16), None, 10),
        pairs(&[("Ctrl+C", 1), ("Ctrl+V", 1)])
    );
    // アプリ名は完全一致で絞り込む。
    assert_eq!(
        ranking(date(15), date(16), Some("EXCEL.EXE"), 10),
        pairs(&[("Ctrl+V", 2), ("Ctrl+C", 1)])
    );
    assert!(ranking(date(15), date(16), Some("Unknown.exe"), 10).is_empty());

    let applications = command_api::application_ranking(&connection, date(15), date(16), 10)
        .expect("application ranking")
        .into_iter()
        .map(|row| (row.application, row.count))
        .collect::<Vec<_>>();
    assert_eq!(applications, pairs(&[("EXCEL.EXE", 3), ("Code.exe", 2)]));
}

#[test]
fn hourly_trend_always_returns_24_buckets_by_recorded_hour() {
    let connection = populated_database();
    let hourly = command_api::hourly_trend(&connection, date(15), date(16)).expect("hourly");

    assert_eq!(
        hourly
            .points
            .iter()
            .map(|point| point.hour)
            .collect::<Vec<_>>(),
        (0..24).collect::<Vec<u8>>()
    );
    let counts: HashMap<u8, i64> = hourly
        .points
        .iter()
        .filter(|point| point.count != 0)
        .map(|point| (point.hour, point.count))
        .collect();
    // 08-14 23:59:59 と 08-17 00:00:00 は期間外なので23時・0時に混ざらない。
    assert_eq!(counts, HashMap::from([(0, 1), (9, 2), (12, 1), (23, 1)]));
    // 時間別集計の保持開始日は期間に関係なくDB全体の最小日付。
    assert_eq!(hourly.first_date.as_deref(), Some("2026-08-14"));
}

#[test]
fn daily_trend_and_detail_return_only_days_with_data() {
    let connection = populated_database();
    let daily = command_api::daily_trend(&connection, date(14), date(17), Some("Ctrl+C"), None)
        .expect("daily trend")
        .into_iter()
        .map(|point| (point.date, point.count))
        .collect::<Vec<_>>();
    assert_eq!(
        daily,
        vec![("2026-08-15".to_owned(), 2), ("2026-08-16".to_owned(), 1)]
    );

    let detail =
        command_api::shortcut_detail(&connection, "Ctrl+C", date(15), date(16)).expect("detail");
    assert_eq!(detail.shortcut, "Ctrl+C");
    assert_eq!(detail.total_count, 3);
    assert_eq!(
        detail
            .applications
            .into_iter()
            .map(|row| (row.application, row.count))
            .collect::<Vec<_>>(),
        vec![("Code.exe".to_owned(), 2), ("EXCEL.EXE".to_owned(), 1)]
    );
    assert_eq!(detail.daily.len(), 2);
}

// ---------------------------------------------------------------------------
// 設定・除外アプリ

#[test]
fn new_database_reports_documented_default_settings() {
    let connection = empty_database();
    let settings = command_api::get_settings(&connection).expect("settings");
    assert!(settings.auto_start);
    assert!(!settings.capture_elevated);
    assert_eq!(settings.theme, "dark");
    assert!(settings.overlay_enabled);
    assert!(!settings.usage_tracking_enabled);
    assert_eq!(settings.duplicate_guard_seconds, 1.0);
    assert_eq!(settings.detail_event_retention_days, 7);
}

#[test]
fn setting_commands_validate_input_and_keep_previous_value_on_rejection() {
    let connection = empty_database();

    command_api::set_theme_command(&connection, "LIGHT").expect("theme");
    assert!(matches!(
        command_api::set_theme_command(&connection, "blue"),
        Err(SettingsError::InvalidTheme(_))
    ));

    command_api::set_duplicate_guard_seconds_command(&connection, 2.34).expect("guard");
    for invalid in [-0.1, 10.1, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                command_api::set_duplicate_guard_seconds_command(&connection, invalid),
                Err(SettingsError::InvalidNumber(_))
            ),
            "{invalid}"
        );
    }

    command_api::set_detail_event_retention_days_command(&connection, 30).expect("retention");
    for invalid in [0, 366] {
        assert!(
            matches!(
                command_api::set_detail_event_retention_days_command(&connection, invalid),
                Err(SettingsError::InvalidNumber(_))
            ),
            "{invalid}"
        );
    }

    command_api::set_usage_tracking_command(&connection, true).expect("usage tracking");
    command_api::set_capture_elevated_command(&connection, true).expect("elevated flag");

    let settings = command_api::get_settings(&connection).expect("settings");
    assert_eq!(settings.theme, "light");
    assert_eq!(settings.duplicate_guard_seconds, 2.3);
    assert_eq!(settings.detail_event_retention_days, 30);
    assert!(settings.usage_tracking_enabled);
    assert!(settings.capture_elevated);
}

#[test]
fn daily_export_preferences_are_normalized_and_validated() {
    let connection = empty_database();
    assert_eq!(
        command_api::daily_export_applications(&connection).expect("default selection"),
        None
    );
    assert_eq!(
        command_api::daily_export_name(&connection).expect("default name"),
        ""
    );

    let selected = ["b.exe", "a.exe", "a.exe"].map(str::to_owned);
    command_api::set_daily_export_applications_command(&connection, &selected).expect("selection");
    assert_eq!(
        command_api::daily_export_applications(&connection).expect("selection"),
        Some(vec!["a.exe".to_owned(), "b.exe".to_owned()])
    );
    assert!(matches!(
        command_api::set_daily_export_applications_command(&connection, &[String::new()]),
        Err(SettingsError::InvalidDailyExportApplications)
    ));

    command_api::set_daily_export_name_command(&connection, "  月次集計  ").expect("name");
    assert_eq!(
        command_api::daily_export_name(&connection).expect("name"),
        "月次集計"
    );
    for invalid in ["改行\n入り".to_owned(), "x".repeat(129)] {
        assert!(matches!(
            command_api::set_daily_export_name_command(&connection, &invalid),
            Err(SettingsError::InvalidDailyExportName)
        ));
    }
    assert_eq!(
        command_api::daily_export_name(&connection).expect("name kept"),
        "月次集計"
    );
}

#[test]
fn excluded_app_changes_are_reflected_in_the_worker_set() {
    let connection = empty_database();
    let shared = shared_excluded_apps(HashSet::new());
    let snapshot = || shared.read().expect("shared set").clone();

    command_api::add_excluded_app_and_reload(&connection, &shared, " Notepad.EXE ").expect("add");
    // worker側は大小文字を区別しない比較のため小文字で保持する。
    assert!(snapshot().contains("notepad.exe"));
    assert!(snapshot().contains("keepass.exe"), "default exclusions");
    let listed = command_api::get_excluded_apps(&connection).expect("list");
    let notepad = listed
        .iter()
        .find(|app| app.executable_name == "Notepad.EXE")
        .expect("user exclusion is stored trimmed");
    assert_eq!(notepad.source, "user");
    assert!(notepad.enabled);

    assert!(command_api::set_excluded_app_enabled_and_reload(
        &connection,
        &shared,
        "notepad.exe",
        false
    )
    .expect("disable"));
    assert!(!snapshot().contains("notepad.exe"));
    assert!(!command_api::set_excluded_app_enabled_and_reload(
        &connection,
        &shared,
        "none.exe",
        false
    )
    .expect("unknown app"));

    // 初期除外アプリはユーザー操作で削除できず、無効化だけができる。
    assert!(
        !command_api::remove_user_excluded_app_and_reload(&connection, &shared, "KeePass.exe")
            .expect("remove default")
    );
    assert!(snapshot().contains("keepass.exe"));
    assert!(
        command_api::remove_user_excluded_app_and_reload(&connection, &shared, "Notepad.EXE")
            .expect("remove user")
    );

    let before = snapshot();
    for invalid in ["", "   ", r"..\evil.exe", "dir/app.exe"] {
        assert!(
            matches!(
                command_api::add_excluded_app_and_reload(&connection, &shared, invalid),
                Err(ExclusionCommandError::Storage(
                    StorageError::InvalidExecutableName
                ))
            ),
            "{invalid:?}"
        );
    }
    assert_eq!(snapshot(), before);
}

// ---------------------------------------------------------------------------
// CSV出力

#[test]
fn daily_export_filter_sources_cover_all_recorded_days_and_applications() {
    let empty = empty_database();
    assert!(command_api::list_daily_export_applications(&empty)
        .expect("applications")
        .is_empty());
    assert_eq!(
        command_api::daily_export_date_bounds(&empty).expect("bounds"),
        (None, None)
    );

    let connection = populated_database();
    assert_eq!(
        command_api::list_daily_export_applications(&connection).expect("applications"),
        ["Code.exe", "EXCEL.EXE", "msedge.exe"].map(str::to_owned)
    );
    assert_eq!(
        command_api::daily_export_date_bounds(&connection).expect("bounds"),
        (Some("2026-08-14".to_owned()), Some("2026-08-17".to_owned()))
    );
}

#[test]
fn daily_csv_applies_application_and_inclusive_date_filters() {
    let connection = populated_database();
    let directory = TempDir::new("daily-filter");
    let selected = vec!["EXCEL.EXE".to_owned()];

    let result = command_api::export_daily_csv_command(
        &connection,
        directory.path(),
        Some(&selected),
        Some(date(15)),
        Some(date(16)),
        None,
    )
    .expect("filtered CSV");
    assert_eq!(result.rows, 3);
    assert_eq!(
        read_csv_lines(&result.path),
        [
            "date,shortcut,application,count",
            "2026-08-15,Ctrl+V,EXCEL.EXE,1",
            "2026-08-16,Ctrl+C,EXCEL.EXE,1",
            "2026-08-16,Ctrl+V,EXCEL.EXE,1",
        ]
    );

    // アプリを1つも選ばない場合はヘッダーだけを出力する（全件出力にはしない）。
    let none_selected = command_api::export_daily_csv_command(
        &connection,
        directory.path(),
        Some(&[]),
        None,
        None,
        Some("none"),
    )
    .expect("empty selection");
    assert_eq!(none_selected.rows, 0);
    assert_eq!(
        read_csv_lines(&none_selected.path),
        ["date,shortcut,application,count"]
    );

    assert!(matches!(
        command_api::export_daily_csv_command(
            &connection,
            directory.path(),
            None,
            Some(date(16)),
            Some(date(15)),
            None,
        ),
        Err(MaintenanceError::InvalidDateRange)
    ));
}

#[test]
fn daily_csv_output_name_cannot_leave_the_output_directory() {
    let connection = populated_database();
    let directory = TempDir::new("daily-name");

    for name in [
        r"..\..\evil",
        "../../evil",
        "C:\\Windows\\evil",
        "a:b*c?d|e\"f<g>",
    ] {
        let result = command_api::export_daily_csv_command(
            &connection,
            directory.path(),
            None,
            None,
            None,
            Some(name),
        )
        .expect("CSV");
        assert_eq!(result.path.parent(), Some(directory.path()), "{name}");
        let file_name = result
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("file name");
        assert!(
            file_name.starts_with("ShortcutKeyCounter_daily_"),
            "{file_name}"
        );
        assert!(file_name.ends_with(".csv"), "{file_name}");
        assert!(
            !file_name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']),
            "{file_name}"
        );
    }
}

#[test]
fn csv_exports_neutralize_formula_like_application_names() {
    // 実行ファイル名は = + - @ で始められるため、Excel等で数式として実行されないこと。
    let mut connection = empty_database();
    let applications = [
        "=HYPERLINK(\"http://example.invalid\",\"x\").exe",
        "+calc.exe",
        "-tool.exe",
        "@SUM(A1).exe",
        "＝全角.exe",
    ];
    for application in applications {
        let event = RecordableEvent {
            occurred_at: NaiveDateTime::parse_from_str("2026-08-15 09:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("timestamp"),
            shortcut: "Ctrl+C".to_owned(),
            application: application.to_owned(),
        };
        record_event(&mut connection, &event).expect("record event");
    }
    let directory = TempDir::new("formula");

    let daily = command_api::export_daily_csv_command(
        &connection,
        directory.path(),
        None,
        None,
        None,
        Some("formula"),
    )
    .expect("daily CSV");
    let events =
        command_api::export_events_csv_command(&connection, directory.path()).expect("events CSV");

    for path in [&daily.path, &events.path] {
        let lines = read_csv_lines(path);
        assert_eq!(lines.len(), applications.len() + 1, "{}", path.display());
        // 日次: date,shortcut,application,count / 詳細: timestamp,shortcut,application
        for line in &lines[1..] {
            let (_, application) = line.split_once(",Ctrl+C,").expect("shortcut column");
            let application = application.strip_prefix('"').unwrap_or(application);
            assert!(application.starts_with('\''), "{}: {line}", path.display());
        }
    }
    let daily_lines = read_csv_lines(&daily.path);
    assert!(daily_lines.contains(&"2026-08-15,Ctrl+C,'+calc.exe,1".to_owned()));
    assert!(daily_lines.contains(
        &"2026-08-15,Ctrl+C,\"'=HYPERLINK(\"\"http://example.invalid\"\",\"\"x\"\").exe\",1"
            .to_owned()
    ));
}

// ---------------------------------------------------------------------------
// キーマッピング編集セッション

fn mapping_values() -> MappingEditValues {
    MappingEditValues {
        mapping: KeyMapping::default(),
        policy: KeyPolicyConfig::default(),
        labels: OverlayKeyLabels::default(),
    }
}

fn mapping_files(directory: &TempDir) -> (PathBuf, OverlayPaths) {
    let root = directory.path();
    (
        root.join("key_mapping.json"),
        OverlayPaths {
            theme_json: root.join("overlay_theme.json"),
            labels_json: root.join("overlay_key_labels.json"),
            policy_json: root.join("overlay_key_policy.json"),
            design_directory: root.join("overlay"),
        },
    )
}

fn saved_mapping(path: &Path) -> KeyMapping {
    serde_json::from_slice(&fs::read(path).expect("mapping JSON")).expect("mapping")
}

#[test]
fn mapping_save_persists_draft_and_clears_unsaved_guard() {
    let directory = TempDir::new("mapping-save");
    let (mapping_path, overlay_paths) = mapping_files(&directory);
    let mut session = MappingEditSession::default();
    session.begin(mapping_values()).expect("begin");

    let mut draft = mapping_values();
    draft.mapping.mouse.left_click = false;
    session.apply_draft(draft).expect("draft");
    assert!(session.snapshot().dirty);
    assert!(!mapping_path.exists(), "draft is not written before save");

    let snapshot = session
        .save_with(|values| save_all_atomic(&mapping_path, &overlay_paths, values))
        .expect("save");
    assert!(snapshot.active);
    assert!(!snapshot.busy);
    assert!(!snapshot.dirty);
    assert!(!snapshot.saved.mapping.mouse.left_click);
    assert!(!saved_mapping(&mapping_path).mouse.left_click);
    assert!(overlay_paths.policy_json.exists());
    assert!(overlay_paths.labels_json.exists());

    session.finish().expect("clean session can finish");
    assert!(!session.snapshot().active);
}

#[test]
fn mapping_save_failure_keeps_saved_values_and_unsaved_guard() {
    let directory = TempDir::new("mapping-save-failure");
    let (mapping_path, overlay_paths) = mapping_files(&directory);
    save_all_atomic(&mapping_path, &overlay_paths, &mapping_values()).expect("initial files");
    let original = fs::read(&mapping_path).expect("original mapping");

    let mut session = MappingEditSession::default();
    session.begin(mapping_values()).expect("begin");
    let mut draft = mapping_values();
    draft.mapping.mouse.right_click = false;
    session.apply_draft(draft).expect("draft");

    // 保存先の親がファイルになっていてディレクトリを作れない状態を作る。
    let blocker = directory.path().join("blocker");
    fs::write(&blocker, b"").expect("blocker file");
    let unwritable = blocker.join("key_mapping.json");
    let error = session
        .save_with(|values| save_all_atomic(&unwritable, &overlay_paths, values))
        .expect_err("save must fail");
    assert!(matches!(error, MappingEditError::Io(_)));

    let snapshot = session.snapshot();
    assert!(!snapshot.busy, "busy is released after failure");
    assert!(snapshot.dirty, "draft is kept for retry");
    assert!(snapshot.saved.mapping.mouse.right_click);
    assert!(matches!(session.finish(), Err(MappingEditError::Dirty)));
    assert_eq!(fs::read(&mapping_path).expect("mapping"), original);

    // 失敗後もそのまま再保存できる。
    session
        .save_with(|values| save_all_atomic(&mapping_path, &overlay_paths, values))
        .expect("retry");
    assert!(!saved_mapping(&mapping_path).mouse.right_click);
}

#[test]
fn mapping_cancel_restores_saved_values_only_after_runtime_rollback() {
    let mut session = MappingEditSession::default();
    session.begin(mapping_values()).expect("begin");
    let mut draft = mapping_values();
    draft.mapping.mouse.middle_click = false;
    session.apply_draft(draft).expect("draft");

    // runtimeへの復元に失敗した場合はdraftを残す。
    let failed = session.cancel_with(|_| Err(MappingEditError::Busy));
    assert!(failed.is_err());
    assert!(session.snapshot().dirty);
    assert!(!session.snapshot().busy);

    let mut restored = None;
    let snapshot = session
        .cancel_with(|saved| {
            restored = Some(saved.clone());
            Ok::<(), MappingEditError>(())
        })
        .expect("cancel");
    assert!(
        restored
            .expect("runtime rollback")
            .mapping
            .mouse
            .middle_click
    );
    assert!(!snapshot.dirty);
    assert_eq!(snapshot.draft, snapshot.saved);
    session.finish().expect("finish after cancel");
}

#[test]
fn mapping_session_state_errors_match_ui_guard_codes() {
    let mut session = MappingEditSession::default();
    let noop = |_: &MappingEditValues| Ok::<(), MappingEditError>(());

    assert!(matches!(
        session.apply_draft(mapping_values()),
        Err(MappingEditError::Inactive)
    ));
    assert!(matches!(
        session.save_with(noop),
        Err(MappingEditError::Inactive)
    ));
    assert!(matches!(
        session.cancel_with(noop),
        Err(MappingEditError::Inactive)
    ));

    session.begin(mapping_values()).expect("begin");
    assert!(matches!(
        session.begin(mapping_values()),
        Err(MappingEditError::AlreadyActive)
    ));

    session.begin_busy().expect("busy");
    assert!(matches!(
        session.apply_draft(mapping_values()),
        Err(MappingEditError::Busy)
    ));
    assert!(matches!(
        session.save_with(noop),
        Err(MappingEditError::Busy)
    ));
    assert!(matches!(session.finish(), Err(MappingEditError::Busy)));
    session.end_busy();

    let mut draft = mapping_values();
    draft.mapping.mouse.x1_click = false;
    session.apply_draft(draft).expect("draft");
    assert!(matches!(session.finish(), Err(MappingEditError::Dirty)));
}

#[test]
fn mapping_draft_cannot_enable_recording_of_unmodified_text_keys() {
    let mut session = MappingEditSession::default();
    session.begin(mapping_values()).expect("begin");
    let mut draft = mapping_values();
    draft.policy.default_single_key = KeyAction::RecordAndDisplayBreak;
    draft.policy.overrides = HashMap::from([
        ("A".to_owned(), KeyAction::RecordAndDisplayBreak),
        ("Num5".to_owned(), KeyAction::RecordAndDisplayBreak),
        ("F5".to_owned(), KeyAction::RecordAndDisplayBreak),
    ]);

    let applied = session.apply_draft(draft).expect("draft");
    // RULE-SEC-001/006: UIから送られたdraftでも修飾なし英数記号は記録へ昇格しない。
    assert_eq!(
        applied.policy.default_single_key,
        KeyAction::ContinueDisplay
    );
    assert_eq!(applied.policy.overrides["A"], KeyAction::ContinueDisplay);
    assert_eq!(applied.policy.overrides["Num5"], KeyAction::ContinueDisplay);
    assert_eq!(
        applied.policy.overrides["F5"],
        KeyAction::RecordAndDisplayBreak
    );
    assert_eq!(session.snapshot().draft.policy, applied.policy);
}
