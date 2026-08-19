//! backend一式を起動し、Tauri UIから制御できるRuntime。

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, select, Receiver, Sender};
use shortcut_common::paths::AppPaths;
use shortcut_windows::power_monitor::{
    start_power_monitor, stop_power_monitor, PowerEvent, PowerMonitorError, PowerMonitorHandle,
};
use thiserror::Error;

use crate::capture_control::{CaptureControlError, CaptureController};
use crate::capture_worker::{
    new_capture_gate, shared_duplicate_guard_threshold, shared_excluded_apps, start_capture_worker,
    CaptureGate, CaptureQueue, CaptureWorkerHandle, SharedDuplicateGuardThreshold,
    SharedExcludedApps, WorkerError, WorkerStats,
};
use crate::database_environment::{DatabaseEnvironment, SharedDatabaseEnvironment};
use crate::diagnostics::{DiagnosticEvent, DiagnosticLogger};
use crate::key_mapping_state::{
    load_or_create, mapping_path, shared as shared_key_mapping, KeyMappingError, SharedKeyMapping,
};
use crate::key_policy::{
    load_or_create as load_key_policy, shared as shared_key_policy, KeyPolicyError, SharedKeyPolicy,
};
use crate::key_search::KeySearchCapture;
use crate::overlay::{OverlayHandle, OverlayRuntime};
use crate::realtime_log::RealtimeLog;
use crate::security_gate::{shared_security_status, SharedSecurityStatus};
use crate::startup::{initialize, StartupContext, StartupError};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("アプリケーション起動初期化に失敗しました: {0}")]
    Startup(#[from] StartupError),
    #[error("capture workerの起動に失敗しました: {0}")]
    Worker(#[from] WorkerError),
    #[error("capture hookの起動に失敗しました: {0}")]
    Capture(#[from] CaptureControlError),
    #[error("power monitorの起動に失敗しました: {0}")]
    Power(#[from] PowerMonitorError),
    #[error("capture lifecycle threadを起動できません: {0}")]
    ThreadStart(#[source] std::io::Error),
    #[error("capture lifecycle threadが異常終了しました")]
    ThreadJoin,
    #[error("capture controllerのstateが壊れています")]
    StatePoisoned,
    #[error("現在の実行ファイルパスを取得できません: {0}")]
    ExecutablePath(#[source] std::io::Error),
    #[error("自動起動設定の同期に失敗しました: {0}")]
    Settings(#[from] crate::settings::SettingsError),
    #[error("キーマッピング初期化に失敗しました: {0}")]
    KeyMapping(#[from] KeyMappingError),
    #[error("キーポリシー初期化に失敗しました: {0}")]
    KeyPolicy(#[from] KeyPolicyError),
    #[error("DB切替に失敗しました: {0}")]
    Storage(#[from] shortcut_storage::StorageError),
}

/// backendの起動後状態。DB writerはworker専用で、UIは別Connectionを使用する。
pub struct CaptureRuntime {
    controller: Arc<Mutex<CaptureController>>,
    queue: CaptureQueue,
    shared_exclusions: SharedExcludedApps,
    key_mapping: SharedKeyMapping,
    key_policy: SharedKeyPolicy,
    realtime_log: RealtimeLog,
    security_status: SharedSecurityStatus,
    overlay: OverlayHandle,
    overlay_runtime: Option<OverlayRuntime>,
    capture_gate: CaptureGate,
    worker: Option<CaptureWorkerHandle>,
    power_monitor: Option<PowerMonitorHandle>,
    stop_sender: Option<Sender<()>>,
    lifecycle_thread: Option<JoinHandle<()>>,
    logger: Arc<DiagnosticLogger>,
    active_database: SharedDatabaseEnvironment,
    duplicate_guard_threshold: SharedDuplicateGuardThreshold,
    key_search: KeySearchCapture,
}

impl CaptureRuntime {
    /// DB初期化完了後にworker、local hooks、power monitorを順に起動する。
    pub fn start() -> Result<(Self, AppPaths), RuntimeError> {
        let context = initialize()?;
        let paths = context.paths.clone();
        let executable_path = std::env::current_exe().map_err(RuntimeError::ExecutablePath)?;
        crate::settings::synchronize_auto_start(&context.connection, executable_path)?;
        let runtime = Self::start_from_context(context)?;
        Ok((runtime, paths))
    }

    /// initialize済みContextから起動する。initializeは呼び直さない。
    pub fn start_from_context(context: StartupContext) -> Result<Self, RuntimeError> {
        let active_database = SharedDatabaseEnvironment::new(&context.paths);
        let key_mapping =
            shared_key_mapping(load_or_create(&mapping_path(&context.paths.app_dir))?);
        let key_policy = shared_key_policy(load_key_policy(
            &context.paths.app_dir.join("overlay_key_policy.json"),
        )?);
        let realtime_log = RealtimeLog::default();
        let duplicate_guard_threshold = shared_duplicate_guard_threshold(
            crate::settings::get_duplicate_guard_seconds(&context.connection).unwrap_or(1.0),
        );
        let key_search = KeySearchCapture::default();
        let overlay_enabled =
            crate::settings::get_overlay_enabled(&context.connection).unwrap_or(true);
        let (overlay_runtime, overlay) =
            OverlayRuntime::start(&context.paths.app_dir, overlay_enabled)
                .unwrap_or_else(|_| OverlayRuntime::disabled(&context.paths.app_dir));
        let StartupContext {
            connection,
            excluded_apps,
            logger,
            ..
        } = context;
        let shared_exclusions = shared_excluded_apps(excluded_apps);
        Self::start_with_context(
            connection,
            shared_exclusions,
            key_mapping,
            key_policy,
            realtime_log,
            overlay_runtime,
            overlay,
            logger,
            active_database,
            duplicate_guard_threshold,
            key_search,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_with_context(
        connection: rusqlite::Connection,
        shared_exclusions: SharedExcludedApps,
        key_mapping: SharedKeyMapping,
        key_policy: SharedKeyPolicy,
        realtime_log: RealtimeLog,
        overlay_runtime: OverlayRuntime,
        overlay: OverlayHandle,
        logger: DiagnosticLogger,
        active_database: SharedDatabaseEnvironment,
        duplicate_guard_threshold: SharedDuplicateGuardThreshold,
        key_search: KeySearchCapture,
    ) -> Result<Self, RuntimeError> {
        let queue = CaptureQueue::new();
        let capture_gate = new_capture_gate();
        let security_status = shared_security_status();
        let worker = start_capture_worker(
            queue.receiver(),
            queue.elevated_receiver(),
            connection,
            Arc::clone(&shared_exclusions),
            Arc::clone(&capture_gate),
            Arc::clone(&key_mapping),
            Arc::clone(&key_policy),
            realtime_log.clone(),
            overlay.clone(),
            Arc::clone(&duplicate_guard_threshold),
            Arc::clone(&security_status),
            key_search.clone(),
        )?;
        let controller = match CaptureController::start(queue.clone()) {
            Ok(controller) => controller,
            Err(error) => {
                drop(worker);
                return Err(error.into());
            }
        };
        let controller = Arc::new(Mutex::new(controller));
        let (power_sender, power_receiver) = bounded(16);
        let power_monitor = match start_power_monitor(power_sender) {
            Ok(handle) => handle,
            Err(error) => {
                if let Ok(mut controller) = controller.lock() {
                    let _ = controller.stop_capture();
                }
                drop(worker);
                return Err(error.into());
            }
        };
        let (stop_sender, stop_receiver) = bounded(1);
        let lifecycle_controller = Arc::clone(&controller);
        let lifecycle_logger = Arc::new(logger);
        let logger_for_runtime = Arc::clone(&lifecycle_logger);
        let lifecycle_thread = thread::Builder::new()
            .name("shortcut-key-capture-lifecycle".to_owned())
            .spawn(move || {
                run_lifecycle(
                    lifecycle_controller,
                    power_receiver,
                    stop_receiver,
                    lifecycle_logger,
                )
            })
            .map_err(RuntimeError::ThreadStart)?;

        Ok(Self {
            controller,
            queue,
            shared_exclusions,
            key_mapping,
            key_policy,
            realtime_log,
            security_status,
            overlay,
            overlay_runtime: Some(overlay_runtime),
            capture_gate,
            worker: Some(worker),
            power_monitor: Some(power_monitor),
            stop_sender: Some(stop_sender),
            lifecycle_thread: Some(lifecycle_thread),
            logger: logger_for_runtime,
            active_database,
            duplicate_guard_threshold,
            key_search,
        })
    }

    pub fn controller(&self) -> Result<std::sync::MutexGuard<'_, CaptureController>, RuntimeError> {
        self.controller
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)
    }

    pub fn controller_handle(&self) -> Arc<Mutex<CaptureController>> {
        Arc::clone(&self.controller)
    }

    pub fn queue(&self) -> CaptureQueue {
        self.queue.clone()
    }

    pub fn shared_exclusions(&self) -> SharedExcludedApps {
        Arc::clone(&self.shared_exclusions)
    }

    pub fn key_mapping(&self) -> SharedKeyMapping {
        Arc::clone(&self.key_mapping)
    }

    pub fn key_policy(&self) -> SharedKeyPolicy {
        Arc::clone(&self.key_policy)
    }

    pub fn realtime_log(&self) -> RealtimeLog {
        self.realtime_log.clone()
    }

    pub fn security_status(&self) -> SharedSecurityStatus {
        Arc::clone(&self.security_status)
    }

    pub fn overlay_handle(&self) -> OverlayHandle {
        self.overlay.clone()
    }

    pub fn logger(&self) -> Arc<DiagnosticLogger> {
        Arc::clone(&self.logger)
    }

    pub fn capture_gate(&self) -> CaptureGate {
        Arc::clone(&self.capture_gate)
    }

    pub fn duplicate_guard_seconds(&self) -> f64 {
        self.duplicate_guard_threshold
            .load(std::sync::atomic::Ordering::Acquire) as f64
            / 1000.0
    }

    pub fn set_duplicate_guard_seconds(&self, seconds: f64) {
        let millis = (seconds.clamp(0.0, 10.0) * 1000.0).round() as u64;
        self.duplicate_guard_threshold
            .store(millis, std::sync::atomic::Ordering::Release);
    }

    pub fn key_search(&self) -> KeySearchCapture {
        self.key_search.clone()
    }

    pub fn active_database(&self) -> SharedDatabaseEnvironment {
        self.active_database.clone()
    }

    pub fn switch_database(
        &mut self,
        environment: DatabaseEnvironment,
    ) -> Result<(), RuntimeError> {
        let old_environment = self.active_database.current();
        if old_environment == environment {
            return Ok(());
        }
        let was_enabled = self.capture_gate_enabled();
        self.set_capture_gate(false);
        self.queue.clear_pending();
        if let Some(worker) = self.worker.take() {
            let _ = worker.stop()?;
        }

        let start_for = |path: &std::path::Path| -> Result<CaptureWorkerHandle, RuntimeError> {
            let mut connection = shortcut_storage::db::open_database(path)?;
            shortcut_storage::migration::run_migrations(&mut connection)?;
            Ok(start_capture_worker(
                self.queue.receiver(),
                self.queue.elevated_receiver(),
                connection,
                Arc::clone(&self.shared_exclusions),
                Arc::clone(&self.capture_gate),
                Arc::clone(&self.key_mapping),
                Arc::clone(&self.key_policy),
                self.realtime_log.clone(),
                self.overlay.clone(),
                Arc::clone(&self.duplicate_guard_threshold),
                Arc::clone(&self.security_status),
                self.key_search.clone(),
            )?)
        };

        let new_path = self.active_database.path_for(environment);
        match start_for(&new_path) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.active_database.set(environment);
                if was_enabled {
                    self.set_capture_gate(true);
                }
                Ok(())
            }
            Err(error) => {
                let old_path = self.active_database.path_for(old_environment);
                self.worker = start_for(&old_path).ok();
                self.active_database.set(old_environment);
                if was_enabled && self.worker.is_some() {
                    self.set_capture_gate(true);
                }
                Err(error)
            }
        }
    }

    pub fn capture_gate_enabled(&self) -> bool {
        self.capture_gate.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn worker_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(CaptureWorkerHandle::is_running)
    }

    pub fn set_capture_gate(&self, enabled: bool) {
        self.capture_gate
            .store(enabled, std::sync::atomic::Ordering::Release);
        if !enabled {
            self.queue.clear_pending();
        }
    }

    /// hooks、power monitor、lifecycle、workerの順に停止する。
    pub fn stop(mut self) -> Result<WorkerStats, RuntimeError> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> Result<WorkerStats, RuntimeError> {
        if self.worker.is_none() {
            return Ok(WorkerStats::default());
        }
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }
        if let Some(power_monitor) = self.power_monitor.take() {
            stop_power_monitor(power_monitor)?;
        }
        if let Some(lifecycle_thread) = self.lifecycle_thread.take() {
            lifecycle_thread
                .join()
                .map_err(|_| RuntimeError::ThreadJoin)?;
        }
        if let Ok(mut controller) = self.controller.lock() {
            controller.stop_capture()?;
        } else {
            return Err(RuntimeError::StatePoisoned);
        }
        let worker = self.worker.take().ok_or(RuntimeError::ThreadJoin)?;
        let stats = worker.stop()?;
        if let Some(overlay_runtime) = self.overlay_runtime.take() {
            overlay_runtime.stop();
        }
        let _ = self.logger.record(DiagnosticEvent::AppStopped);
        Ok(stats)
    }
}

impl Drop for CaptureRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn run_lifecycle(
    controller: Arc<Mutex<CaptureController>>,
    power_receiver: Receiver<PowerEvent>,
    stop_receiver: Receiver<()>,
    logger: Arc<DiagnosticLogger>,
) {
    loop {
        select! {
            recv(stop_receiver) -> _ => break,
            recv(power_receiver) -> event => {
                if matches!(event, Ok(PowerEvent::Resume)) {
                    let _ = logger.record(DiagnosticEvent::PowerResumed);
                    if let Ok(mut controller) = controller.lock() {
                        if controller.recover_capture_after_resume().is_err() {
                            let _ = logger.record(DiagnosticEvent::HookFailed {
                                kind: crate::diagnostics::HookKind::Keyboard,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_error_is_available_without_starting_desktop_hooks() {
        let error = RuntimeError::StatePoisoned;
        assert!(error.to_string().contains("controller"));
    }

    #[test]
    #[ignore = "Windowsデスクトップ上でUIなしのbackend起動と停止を確認する"]
    fn runtime_can_start_and_stop_without_ui() {
        let (runtime, _paths) = CaptureRuntime::start().expect("runtime start");
        let stats = runtime.stop().expect("runtime stop");
        assert_eq!(stats, WorkerStats::default());
    }
}
