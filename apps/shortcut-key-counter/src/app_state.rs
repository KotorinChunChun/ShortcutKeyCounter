//! Tauriが保持するアプリケーションstate。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::Connection;
use shortcut_common::paths::AppPaths;
use shortcut_storage::db::open_database;
use thiserror::Error;

use crate::app_usage::{AppUsageRuntime, UsageTrackingEnabled};
use crate::capture_health::{
    capture_health_snapshot, new_capture_health, update_capture_health, CaptureHealthSnapshot,
    CaptureSource, HealthLevel, SharedCaptureHealth,
};
use crate::capture_worker::SharedExcludedApps;
use crate::database_environment::{
    DatabaseEnvironment, DatabaseEnvironmentSnapshot, SharedDatabaseEnvironment,
};
use crate::elevated_capture::{ElevatedCaptureError, ElevatedCaptureManager};
use crate::key_mapping_state::{KeyMappingError, SharedKeyMapping};
use crate::key_policy::{self, KeyAction, KeyPolicyConfig, KeyPolicyError, SharedKeyPolicy};
use crate::key_search::KeySearchCapture;
use crate::mapping_edit_session::{
    save_all_atomic, MappingEditError, MappingEditSession, MappingEditSessionSnapshot,
    MappingEditValues, MappingEditValuesDto,
};
use crate::overlay::config::{
    load_labels, load_theme, save_labels_atomic, save_theme_atomic, KeyboardInputMode,
    KeyboardLayout, OverlayConfig, OverlayConfigError, OverlayKeyLabels,
};
use crate::overlay::OverlayHandle;
use crate::realtime_log::RealtimeLog;
use crate::runtime::{CaptureRuntime, RuntimeError};
use crate::security_gate::{SecurityStopReason, SharedSecurityStatus};
use crate::settings::{
    get_capture_elevated, get_usage_tracking_enabled, set_capture_elevated,
    set_overlay_enabled as persist_overlay_enabled, set_usage_tracking_enabled, SettingsError,
};

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("backend runtime初期化に失敗しました: {0}")]
    Runtime(#[from] RuntimeError),
    #[error("UI用DB接続を開けません: {0}")]
    Storage(#[from] shortcut_storage::StorageError),
    #[error("設定処理に失敗しました: {0}")]
    Settings(#[from] SettingsError),
    #[error("elevated captureの切替に失敗しました: {0}")]
    Elevated(#[from] ElevatedCaptureError),
    #[error("アプリ内部stateが壊れています")]
    Poisoned,
    #[error("現在の実行ファイルパスを取得できません: {0}")]
    ExecutablePath(#[source] std::io::Error),
    #[error("オーバーレイ設定を処理できません: {0}")]
    OverlayConfig(#[from] OverlayConfigError),
    #[error("キーポリシーを処理できません: {0}")]
    KeyPolicy(#[from] KeyPolicyError),
    #[error("キーマッピングを処理できません: {0}")]
    KeyMapping(#[from] KeyMappingError),
    #[error("キーマッピング編集セッションを処理できません: {0}")]
    MappingEdit(#[from] MappingEditError),
    #[error("Security Gateが保護中です")]
    SecurityGateBlocked,
}

pub struct AppState {
    pub runtime: Mutex<CaptureRuntime>,
    /// Active statistics/capture DB. Swapped atomically between production and test.
    pub database: Mutex<Connection>,
    /// Operational settings/exclusions always live in the production DB.
    pub settings_database: Mutex<Connection>,
    pub active_database: SharedDatabaseEnvironment,
    pub usage_tracking_enabled: UsageTrackingEnabled,
    pub usage_runtime: Mutex<AppUsageRuntime>,
    pub paths: AppPaths,
    pub shared_exclusions: SharedExcludedApps,
    pub key_mapping: SharedKeyMapping,
    pub key_policy: SharedKeyPolicy,
    pub mapping_edit_session: Mutex<MappingEditSession>,
    pub key_search: KeySearchCapture,
    pub realtime_log: RealtimeLog,
    pub security_status: SharedSecurityStatus,
    pub overlay: OverlayHandle,
    pub elevated: Mutex<ElevatedCaptureManager>,
    pub health: SharedCaptureHealth,
    pub exiting: AtomicBool,
}

impl AppState {
    pub fn initialize() -> Result<Self, AppStateError> {
        let (runtime, paths) = CaptureRuntime::start()?;
        let shared_exclusions = runtime.shared_exclusions();
        let key_mapping = runtime.key_mapping();
        let key_policy = runtime.key_policy();
        let key_search = runtime.key_search();
        let realtime_log = runtime.realtime_log();
        let security_status = runtime.security_status();
        let overlay = runtime.overlay_handle();
        let active_database = runtime.active_database();
        let database = open_database(&paths.database_path)?;
        let settings_database = open_database(&paths.database_path)?;
        let usage_tracking_enabled = Arc::new(AtomicBool::new(
            get_usage_tracking_enabled(&settings_database).unwrap_or(false),
        ));
        let usage_runtime =
            AppUsageRuntime::start(active_database.clone(), Arc::clone(&usage_tracking_enabled));
        Ok(Self {
            runtime: Mutex::new(runtime),
            database: Mutex::new(database),
            settings_database: Mutex::new(settings_database),
            active_database,
            usage_tracking_enabled,
            usage_runtime: Mutex::new(usage_runtime),
            paths,
            shared_exclusions,
            key_mapping,
            key_policy,
            mapping_edit_session: Mutex::new(MappingEditSession::default()),
            key_search,
            realtime_log,
            security_status,
            overlay,
            elevated: Mutex::new(ElevatedCaptureManager::new()),
            health: new_capture_health(),
            exiting: AtomicBool::new(false),
        })
    }

    pub fn helper_path(&self) -> Result<PathBuf, AppStateError> {
        let executable = std::env::current_exe().map_err(AppStateError::ExecutablePath)?;
        Ok(executable
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("ShortcutKeyCapture.exe"))
    }

    pub fn capture_enabled(&self) -> Result<bool, AppStateError> {
        Ok(self
            .runtime
            .lock()
            .map_err(|_| AppStateError::Poisoned)?
            .capture_gate_enabled())
    }

    pub fn dropped_queue_items(&self) -> Result<u64, AppStateError> {
        Ok(self
            .runtime
            .lock()
            .map_err(|_| AppStateError::Poisoned)?
            .queue()
            .dropped_count())
    }

    pub fn capture_worker_running(&self) -> Result<bool, AppStateError> {
        Ok(self
            .runtime
            .lock()
            .map_err(|_| AppStateError::Poisoned)?
            .worker_running())
    }

    /// 計測ON/OFFは永続化しない。Elevated中も通常Hookを同じ状態へ同期する。
    pub fn set_capture_enabled(&self, enabled: bool) -> Result<(), AppStateError> {
        let runtime = self.runtime.lock().map_err(|_| AppStateError::Poisoned)?;
        let controller = runtime.controller_handle();

        if enabled {
            controller
                .lock()
                .map_err(|_| AppStateError::Poisoned)?
                .start_capture()
                .map_err(RuntimeError::from)?;
            runtime.set_capture_gate(true);
        } else {
            runtime.set_capture_gate(false);
            controller
                .lock()
                .map_err(|_| AppStateError::Poisoned)?
                .stop_capture()
                .map_err(RuntimeError::from)?;
        }
        Ok(())
    }

    /// Overlay設定はDB保存成功後にruntimeへ反映する。Overlay失敗はCapture本体へ波及させない。
    pub fn set_overlay_enabled(&self, enabled: bool) -> Result<(), AppStateError> {
        {
            let database = self
                .settings_database
                .lock()
                .map_err(|_| AppStateError::Poisoned)?;
            persist_overlay_enabled(&database, enabled)?;
        }
        self.overlay.set_enabled(enabled);
        Ok(())
    }

    pub fn overlay_enabled(&self) -> Result<bool, AppStateError> {
        let database = self
            .settings_database
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        Ok(crate::settings::get_overlay_enabled(&database)?)
    }

    pub fn overlay_config(&self) -> OverlayConfig {
        load_theme(self.overlay.theme_path())
    }

    pub fn key_policy_snapshot(&self) -> Result<KeyPolicyConfig, AppStateError> {
        Ok(key_policy::snapshot(&self.key_policy)?)
    }

    pub fn overlay_labels(&self) -> OverlayKeyLabels {
        load_labels(self.overlay.labels_path())
    }

    /// 保存前のOverlay外観draftをPreviewする。Security Gateがblocked/unknownの間は
    /// Previewを拒否し、既に表示中のPreviewもcapture workerと同じreset経路で消す。
    pub fn preview_overlay_settings(
        &self,
        config: OverlayConfig,
        generation: u64,
    ) -> Result<(), AppStateError> {
        let blocked = self
            .security_status
            .read()
            .map_err(|_| AppStateError::Poisoned)
            .map(|status| status.blocked || status.reason == SecurityStopReason::Unknown)?;
        if blocked {
            self.overlay.reset_session();
            return Err(AppStateError::SecurityGateBlocked);
        }
        self.overlay.preview_draft(config.sanitize(), generation);
        Ok(())
    }

    /// 外観関連だけを既定値へ戻し、保存成功後にOverlay runtimeへ反映する。
    pub fn reset_overlay_appearance(&self) -> Result<OverlayConfig, AppStateError> {
        let config = self.overlay_config().reset_appearance_from_defaults();
        Ok(self.overlay.save_settings(&config)?)
    }

    pub fn replace_overlay_labels(
        &self,
        labels: OverlayKeyLabels,
    ) -> Result<OverlayKeyLabels, AppStateError> {
        let labels = OverlayKeyLabels::merge_with_defaults(labels);
        save_labels_atomic(self.overlay.labels_path(), &labels)?;
        self.overlay.reload_assets();
        Ok(labels)
    }

    pub fn replace_overlay_keyboard_layout(
        &self,
        layout: KeyboardLayout,
    ) -> Result<OverlayKeyLabels, AppStateError> {
        self.replace_overlay_labels(self.overlay_labels().with_layout(layout))
    }

    /// Overlayの表示設定、単独キー既定ポリシー、ON/OFFを一括保存する。
    /// ファイルまたはSQLiteの保存に失敗した場合は、可能な範囲で変更前へ戻す。
    pub fn apply_overlay_settings(
        &self,
        config: OverlayConfig,
        enabled: bool,
        default_single_key_action: KeyAction,
    ) -> Result<OverlayConfig, AppStateError> {
        let new_config = config.sanitize();
        let old_config = self.overlay_config();
        let old_policy = self.key_policy_snapshot()?;
        let old_enabled = self.overlay_enabled()?;
        let mut new_policy = old_policy.clone();
        new_policy.default_single_key = default_single_key_action;
        new_policy = new_policy.sanitize();

        save_theme_atomic(self.overlay.theme_path(), &new_config)?;
        if let Err(error) = key_policy::save_atomic(self.overlay.policy_path(), &new_policy) {
            let _ = save_theme_atomic(self.overlay.theme_path(), &old_config);
            return Err(error.into());
        }
        if let Err(error) = {
            let database = self
                .settings_database
                .lock()
                .map_err(|_| AppStateError::Poisoned)?;
            persist_overlay_enabled(&database, enabled)
        } {
            let _ = save_theme_atomic(self.overlay.theme_path(), &old_config);
            let _ = key_policy::save_atomic(self.overlay.policy_path(), &old_policy);
            return Err(error.into());
        }
        if let Err(error) = self
            .key_policy
            .write()
            .map_err(|_| AppStateError::KeyPolicy(KeyPolicyError::Poisoned))
            .map(|mut shared| *shared = new_policy)
        {
            let _ = save_theme_atomic(self.overlay.theme_path(), &old_config);
            let _ = key_policy::save_atomic(self.overlay.policy_path(), &old_policy);
            if let Ok(database) = self.settings_database.lock() {
                let _ = persist_overlay_enabled(&database, old_enabled);
            }
            return Err(error);
        }

        self.overlay.set_enabled(enabled);
        self.overlay.reload_assets();
        Ok(new_config)
    }

    pub fn replace_key_policy(
        &self,
        config: KeyPolicyConfig,
    ) -> Result<KeyPolicyConfig, AppStateError> {
        Ok(key_policy::replace_and_save(
            self.overlay.policy_path(),
            &self.key_policy,
            config,
        )?)
    }

    pub fn reload_overlay_assets(&self) {
        self.overlay.reload_assets();
        if let Ok(policy) = key_policy::load_or_create(self.overlay.policy_path()) {
            if let Ok(mut shared) = self.key_policy.write() {
                *shared = policy;
            }
        }
    }

    fn apply_mapping_runtime(&self, values: &MappingEditValues) -> Result<(), AppStateError> {
        *self
            .key_mapping
            .write()
            .map_err(|_| AppStateError::KeyMapping(KeyMappingError::Poisoned))? =
            values.mapping.clone();
        *self
            .key_policy
            .write()
            .map_err(|_| AppStateError::KeyPolicy(KeyPolicyError::Poisoned))? =
            values.policy.clone();
        self.overlay
            .set_labels_override(Some(values.labels.clone()));
        Ok(())
    }

    pub fn begin_mapping_edit(&self) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let values = MappingEditValues {
            mapping: crate::key_mapping_state::snapshot(&self.key_mapping)?,
            policy: key_policy::snapshot(&self.key_policy)?,
            labels: self.overlay_labels(),
        };
        let mut session = self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        Ok(session.begin(values)?)
    }

    pub fn mapping_edit_snapshot(&self) -> Result<MappingEditSessionSnapshot, AppStateError> {
        Ok(self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?
            .snapshot())
    }

    pub fn apply_mapping_edit_draft(
        &self,
        values: MappingEditValuesDto,
    ) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut session = self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        let values = session.apply_draft(values.into())?;
        self.apply_mapping_runtime(&values)?;
        Ok(session.snapshot())
    }

    pub fn replace_overlay_keyboard_input_mode(
        &self,
        mode: KeyboardInputMode,
    ) -> Result<OverlayKeyLabels, AppStateError> {
        self.replace_overlay_labels(self.overlay_labels().with_input_mode(mode))
    }

    pub fn apply_mapping_edit_layout(
        &self,
        values: MappingEditValuesDto,
        layout: KeyboardLayout,
    ) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut values: MappingEditValues = values.into();
        values.labels = values.labels.with_layout(layout);
        self.apply_mapping_edit_draft(MappingEditValuesDto::from(&values))
    }

    pub fn apply_mapping_edit_input_mode(
        &self,
        values: MappingEditValuesDto,
        mode: KeyboardInputMode,
    ) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut values: MappingEditValues = values.into();
        values.labels = values.labels.with_input_mode(mode);
        self.apply_mapping_edit_draft(MappingEditValuesDto::from(&values))
    }

    /// Window Close / process ExitRequested / Tray終了の共通ガード。
    /// state取得不能時は安全側（ブロック）に倒す。
    pub fn mapping_edit_is_blocking(&self) -> bool {
        self.mapping_edit_session
            .lock()
            .map(|session| {
                let snapshot = session.snapshot();
                snapshot.dirty || snapshot.busy
            })
            .unwrap_or(true)
    }

    pub fn save_mapping_edit(&self) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut session = self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        session.begin_busy()?;
        let values = session.draft()?;
        let result = save_all_atomic(
            &crate::key_mapping_state::mapping_path(&self.paths.app_dir),
            &crate::overlay::config::OverlayPaths {
                theme_json: self.overlay.theme_path().to_path_buf(),
                labels_json: self.overlay.labels_path().to_path_buf(),
                policy_json: self.overlay.policy_path().to_path_buf(),
                design_directory: self.overlay.design_directory().to_path_buf(),
            },
            &values,
        );
        if let Err(error) = result {
            session.end_busy();
            return Err(error.into());
        }
        session.mark_saved()?;
        session.end_busy();
        self.overlay.set_labels_override(None);
        Ok(session.snapshot())
    }

    pub fn cancel_mapping_edit(&self) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut session = self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        session.begin_busy()?;
        let values = session.saved()?;
        if let Err(error) = self.apply_mapping_runtime(&values) {
            session.end_busy();
            return Err(error);
        }
        session.restore_saved()?;
        session.end_busy();
        self.overlay.set_labels_override(None);
        Ok(session.snapshot())
    }

    pub fn end_mapping_edit(&self) -> Result<MappingEditSessionSnapshot, AppStateError> {
        let mut session = self
            .mapping_edit_session
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        session.finish()?;
        self.overlay.set_labels_override(None);
        Ok(session.snapshot())
    }

    pub fn capture_health(&self) -> CaptureHealthSnapshot {
        capture_health_snapshot(&self.health)
    }

    /// 実際のCapture経路切替成功後だけ設定DBを更新する。
    /// runtime MutexをUAC/Pipe待機中に保持しないことで、UI/Trayの状態取得を塞がない。
    pub fn set_elevated_enabled(&self, enabled: bool) -> Result<(), AppStateError> {
        let (queue, gate, logger) = {
            let runtime = self.runtime.lock().map_err(|_| AppStateError::Poisoned)?;
            (runtime.queue(), runtime.capture_gate(), runtime.logger())
        };
        let mut elevated = self.elevated.lock().map_err(|_| AppStateError::Poisoned)?;

        if enabled {
            let helper_path = self.helper_path()?;
            elevated.start(helper_path, queue, gate, self.health.clone(), logger)?;
        } else {
            elevated.stop(self.health.clone())?;
        }
        drop(elevated);

        let database = self
            .settings_database
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        if let Err(error) = set_capture_elevated(&database, enabled) {
            update_capture_health(
                &self.health,
                CaptureHealthSnapshot {
                    source: self.capture_health().source,
                    level: HealthLevel::Degraded,
                    message: Some(
                        "Capture経路は切り替わりましたが設定の保存に失敗しました。".to_owned(),
                    ),
                },
            );
            return Err(error.into());
        }
        Ok(())
    }

    /// 起動時の永続設定がONなら一度だけhelper起動を試す。失敗時はlocal captureを継続する。
    pub fn restore_elevated_setting(&self) -> Result<(), AppStateError> {
        let enabled = {
            let database = self
                .settings_database
                .lock()
                .map_err(|_| AppStateError::Poisoned)?;
            get_capture_elevated(&database)?
        };
        if !enabled {
            return Ok(());
        }
        if let Err(error) = self.set_elevated_enabled(true) {
            update_capture_health(
                &self.health,
                CaptureHealthSnapshot {
                    source: CaptureSource::LocalFallback,
                    level: HealthLevel::Degraded,
                    message: Some(format!(
                        "管理者権限Captureを開始できなかったため通常権限で計測しています: {error}"
                    )),
                },
            );
        }
        Ok(())
    }

    pub fn database_environment(&self) -> DatabaseEnvironmentSnapshot {
        self.active_database.snapshot()
    }

    pub fn switch_database(
        &self,
        environment: DatabaseEnvironment,
    ) -> Result<DatabaseEnvironmentSnapshot, AppStateError> {
        let target_path = self.active_database.path_for(environment);
        let mut ui_connection = open_database(&target_path)?;
        shortcut_storage::migration::run_migrations(&mut ui_connection)?;
        {
            let mut runtime = self.runtime.lock().map_err(|_| AppStateError::Poisoned)?;
            runtime.switch_database(environment)?;
        }
        *self.database.lock().map_err(|_| AppStateError::Poisoned)? = ui_connection;
        self.realtime_log.clear();
        Ok(self.active_database.snapshot())
    }

    pub fn set_usage_tracking(&self, enabled: bool) -> Result<(), AppStateError> {
        {
            let database = self
                .settings_database
                .lock()
                .map_err(|_| AppStateError::Poisoned)?;
            set_usage_tracking_enabled(&database, enabled)?;
        }
        self.usage_tracking_enabled
            .store(enabled, Ordering::Release);
        Ok(())
    }

    /// DuplicateGuard設定を永続化し、Capture workerへ次イベントから反映する。
    pub fn set_duplicate_guard_seconds(&self, seconds: f64) -> Result<(), AppStateError> {
        let persisted_seconds = {
            let database = self
                .settings_database
                .lock()
                .map_err(|_| AppStateError::Poisoned)?;
            crate::settings::set_duplicate_guard_seconds(&database, seconds)?;
            crate::settings::get_duplicate_guard_seconds(&database)?
        };
        self.runtime
            .lock()
            .map_err(|_| AppStateError::Poisoned)?
            .set_duplicate_guard_seconds(persisted_seconds);
        Ok(())
    }

    /// 詳細ショートカット履歴の保持日数は次回起動時のretention cleanupから使用する。
    pub fn set_detail_event_retention_days(&self, days: u32) -> Result<(), AppStateError> {
        let database = self
            .settings_database
            .lock()
            .map_err(|_| AppStateError::Poisoned)?;
        crate::settings::set_detail_event_retention_days(&database, days)?;
        Ok(())
    }

    pub fn mark_exiting(&self) {
        self.exiting.store(true, Ordering::Release);
        if let Ok(mut usage) = self.usage_runtime.lock() {
            usage.stop();
        }
    }

    pub fn is_exiting(&self) -> bool {
        self.exiting.load(Ordering::Acquire)
    }
}
