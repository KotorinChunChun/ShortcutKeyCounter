use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, select, Receiver, Sender, TrySendError};
use rusqlite::Connection;
use shortcut_core::duplicate_guard::DuplicateGuard;
use shortcut_core::key_mapping::KeyMapping;
use shortcut_core::key_names::{is_modifier_key, normalize_single_key};
use shortcut_core::model::{CapturedInput, MainInput, RecordableEvent};
use shortcut_core::normalizer::normalize_input;
use shortcut_storage::{record_event, StorageError};
use thiserror::Error;

use crate::key_mapping_state::SharedKeyMapping;
use crate::key_policy::{KeyAction, KeyDecision, KeyPolicyConfig, SharedKeyPolicy};
use crate::key_search::KeySearchCapture;
#[cfg(test)]
use crate::overlay::OverlayRuntime;
use crate::overlay::{OverlayCaptureSource, OverlayEvent, OverlayHandle, OverlayInputKind};
use crate::realtime_log::RealtimeLog;
use crate::security_gate::{SecurityGate, SharedSecurityStatus};

pub const CAPTURE_QUEUE_CAPACITY: usize = 1024;

/// 稼働中のCapture workerと設定UIで共有する除外アプリ集合。
pub type SharedExcludedApps = Arc<RwLock<HashSet<String>>>;
/// 計測ON/OFFをworkerまで即時反映する非永続gate。
pub type CaptureGate = Arc<AtomicBool>;
/// DuplicateGuardの閾値(ms)をworkerと設定UIで共有する。
pub type SharedDuplicateGuardThreshold = Arc<AtomicU64>;

pub fn shared_duplicate_guard_threshold(seconds: f64) -> SharedDuplicateGuardThreshold {
    let millis = (seconds.clamp(0.0, 10.0) * 1000.0).round() as u64;
    Arc::new(AtomicU64::new(millis))
}

pub fn new_capture_gate() -> CaptureGate {
    Arc::new(AtomicBool::new(true))
}

/// DBから読み込んだ除外一覧を大小文字非依存で共有できる形へ変換する。
pub fn shared_excluded_apps(excluded_apps: HashSet<String>) -> SharedExcludedApps {
    Arc::new(RwLock::new(
        excluded_apps
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect(),
    ))
}

/// Hook callbackとElevated helperからworkerへ渡すbounded queue。
/// Local/Elevatedを別channelにして、Overlay側だけ同一物理入力の短時間重複除去ができるようにする。
#[derive(Clone)]
pub struct CaptureQueue {
    local_sender: Sender<CapturedInput>,
    local_receiver: Receiver<CapturedInput>,
    elevated_sender: Sender<CapturedInput>,
    elevated_receiver: Receiver<CapturedInput>,
    dropped_count: Arc<AtomicU64>,
}

impl CaptureQueue {
    pub fn new() -> Self {
        Self::with_capacity(CAPTURE_QUEUE_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (local_sender, local_receiver) = bounded(capacity);
        let (elevated_sender, elevated_receiver) = bounded(capacity);
        Self {
            local_sender,
            local_receiver,
            elevated_sender,
            elevated_receiver,
            dropped_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 通常権限Hookへ渡すsender。
    pub fn sender(&self) -> Sender<CapturedInput> {
        self.local_sender.clone()
    }

    pub fn receiver(&self) -> Receiver<CapturedInput> {
        self.local_receiver.clone()
    }

    pub fn elevated_receiver(&self) -> Receiver<CapturedInput> {
        self.elevated_receiver.clone()
    }

    /// 通常権限Hook callbackから呼び出す。満杯時もblockしない。
    pub fn try_send(&self, input: CapturedInput) -> Result<(), TrySendError<CapturedInput>> {
        try_send_counted(&self.local_sender, &self.dropped_count, input)
    }

    /// 管理者権限helperから受信した入力専用。Localと経路を分けてOverlay重複除去に利用する。
    pub fn try_send_elevated(
        &self,
        input: CapturedInput,
    ) -> Result<(), TrySendError<CapturedInput>> {
        try_send_counted(&self.elevated_sender, &self.dropped_count, input)
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// OFFへ切り替える際に、まだworkerへ渡っていない入力を両経路とも破棄する。
    pub fn clear_pending(&self) {
        while self.local_receiver.try_recv().is_ok() {}
        while self.elevated_receiver.try_recv().is_ok() {}
    }
}

fn try_send_counted(
    sender: &Sender<CapturedInput>,
    dropped_count: &AtomicU64,
    input: CapturedInput,
) -> Result<(), TrySendError<CapturedInput>> {
    match sender.try_send(input) {
        Ok(()) => Ok(()),
        Err(error @ TrySendError::Full(_)) => {
            dropped_count.fetch_add(1, Ordering::Relaxed);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

impl Default for CaptureQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct WorkerStats {
    pub recorded: u64,
    pub dropped_ineligible: u64,
    pub dropped_unsupported: u64,
    pub dropped_excluded: u64,
    pub dropped_duplicate: u64,
    pub dropped_invalid_application: u64,
    pub dropped_mapping: u64,
    pub displayed_only: u64,
    pub dropped_ignored: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    Recorded,
    DroppedIneligible,
    DroppedUnsupported,
    DroppedExcluded,
    DroppedDuplicate,
    DroppedInvalidApplication,
    DroppedMapping,
    DisplayedOnly,
    DroppedIgnored,
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("capture workerを起動できません: {0}")]
    ThreadStart(#[source] std::io::Error),
    #[error("capture workerのDB処理に失敗しました: {0}")]
    Storage(#[from] StorageError),
    #[error("capture worker threadが異常終了しました")]
    ThreadJoin,
    #[error("除外アプリ共有stateが壊れています")]
    ExclusionStatePoisoned,
    #[error("キーマッピング共有stateが壊れています")]
    KeyMappingStatePoisoned,
    #[error("キーポリシー共有stateが壊れています")]
    KeyPolicyStatePoisoned,
}

pub struct CaptureWorkerHandle {
    stop_sender: Option<Sender<()>>,
    join_handle: Option<JoinHandle<Result<WorkerStats, WorkerError>>>,
}

/// Capture workerを起動する。DB writerはこのthreadだけに限定する。
#[allow(clippy::too_many_arguments)]
pub fn start_capture_worker(
    local_receiver: Receiver<CapturedInput>,
    elevated_receiver: Receiver<CapturedInput>,
    connection: Connection,
    excluded_apps: SharedExcludedApps,
    capture_gate: CaptureGate,
    key_mapping: SharedKeyMapping,
    key_policy: SharedKeyPolicy,
    realtime_log: RealtimeLog,
    overlay: OverlayHandle,
    duplicate_guard_threshold: SharedDuplicateGuardThreshold,
    security_status: SharedSecurityStatus,
    key_search: KeySearchCapture,
) -> Result<CaptureWorkerHandle, WorkerError> {
    let (stop_sender, stop_receiver) = bounded(1);
    let join_handle = thread::Builder::new()
        .name("shortcut-key-capture-worker".to_owned())
        .spawn(move || {
            run_worker(
                local_receiver,
                elevated_receiver,
                stop_receiver,
                connection,
                excluded_apps,
                capture_gate,
                key_mapping,
                key_policy,
                realtime_log,
                overlay,
                duplicate_guard_threshold,
                security_status,
                key_search,
            )
        })
        .map_err(WorkerError::ThreadStart)?;

    Ok(CaptureWorkerHandle {
        stop_sender: Some(stop_sender),
        join_handle: Some(join_handle),
    })
}

impl CaptureWorkerHandle {
    pub fn is_running(&self) -> bool {
        self.join_handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    pub fn stop(mut self) -> Result<WorkerStats, WorkerError> {
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }
        self.join()
    }

    fn join(&mut self) -> Result<WorkerStats, WorkerError> {
        self.join_handle
            .take()
            .ok_or(WorkerError::ThreadJoin)?
            .join()
            .map_err(|_| WorkerError::ThreadJoin)?
    }
}

impl Drop for CaptureWorkerHandle {
    fn drop(&mut self) {
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    local_receiver: Receiver<CapturedInput>,
    elevated_receiver: Receiver<CapturedInput>,
    stop_receiver: Receiver<()>,
    mut connection: Connection,
    excluded_apps: SharedExcludedApps,
    capture_gate: CaptureGate,
    key_mapping: SharedKeyMapping,
    key_policy: SharedKeyPolicy,
    realtime_log: RealtimeLog,
    overlay: OverlayHandle,
    duplicate_guard_threshold: SharedDuplicateGuardThreshold,
    security_status: SharedSecurityStatus,
    key_search: KeySearchCapture,
) -> Result<WorkerStats, WorkerError> {
    let security_gate = SecurityGate::new(security_status);
    let mut duplicate_guard = DuplicateGuard::new();
    let mut stats = WorkerStats::default();

    loop {
        select! {
            recv(stop_receiver) -> _ => break,
            recv(local_receiver) -> received => match received {
                Ok(input) => process_worker_input(
                    &mut connection,
                    &mut duplicate_guard,
                    &excluded_apps,
                    &capture_gate,
                    &key_mapping,
                    &key_policy,
                    &realtime_log,
                    &overlay,
                    &duplicate_guard_threshold,
                    &key_search,
                    OverlayCaptureSource::Local,
                    input,
                    &mut stats,
                    &security_gate,
                )?,
                Err(_) => break,
            },
            recv(elevated_receiver) -> received => match received {
                Ok(input) => process_worker_input(
                    &mut connection,
                    &mut duplicate_guard,
                    &excluded_apps,
                    &capture_gate,
                    &key_mapping,
                    &key_policy,
                    &realtime_log,
                    &overlay,
                    &duplicate_guard_threshold,
                    &key_search,
                    OverlayCaptureSource::Elevated,
                    input,
                    &mut stats,
                    &security_gate,
                )?,
                Err(_) => break,
            }
        }
    }

    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn process_worker_input(
    connection: &mut Connection,
    duplicate_guard: &mut DuplicateGuard,
    excluded_apps: &SharedExcludedApps,
    capture_gate: &CaptureGate,
    key_mapping: &SharedKeyMapping,
    key_policy: &SharedKeyPolicy,
    realtime_log: &RealtimeLog,
    overlay: &OverlayHandle,
    duplicate_guard_threshold: &SharedDuplicateGuardThreshold,
    key_search: &KeySearchCapture,
    source: OverlayCaptureSource,
    input: CapturedInput,
    stats: &mut WorkerStats,
    security_gate: &SecurityGate,
) -> Result<(), WorkerError> {
    if !capture_gate.load(Ordering::Acquire) {
        return Ok(());
    }
    // RULE-SEC-003/004/007: Security Gate runs before exclusion, Overlay, policy and DB.
    if !security_gate.allows_input() {
        // Security Gate中は既存Overlay sessionも破棄し、repeat runを保護領域の
        // 前後で継続させない。入力内容自体はOverlay threadへ送らない。
        overlay.reset_session();
        return Ok(());
    }
    // UI検索専用Capture mode。ここで消費した入力はOverlay/DB/Realtime/統計へ進めない。
    if key_search.consume_if_armed(&input) {
        return Ok(());
    }
    let excluded_snapshot = excluded_apps
        .read()
        .map_err(|_| WorkerError::ExclusionStatePoisoned)?
        .clone();
    if excluded_snapshot.contains(&input.foreground_executable.to_ascii_lowercase()) {
        stats.record(ProcessOutcome::DroppedExcluded);
        return Ok(());
    }
    // RULE-UI-012/013: detect the retreat click by global pointer coordinates while
    // preserving click-through. A retreat click is not a shortcut-statistics event.
    if overlay.consume_reposition_click(&input) {
        return Ok(());
    }
    duplicate_guard.set_threshold(Duration::from_millis(
        duplicate_guard_threshold
            .load(Ordering::Acquire)
            .min(10_000),
    ));
    let mapping_snapshot = key_mapping
        .read()
        .map_err(|_| WorkerError::KeyMappingStatePoisoned)?
        .clone();
    let trace = process_captured_input_detailed(
        connection,
        duplicate_guard,
        &excluded_snapshot,
        &mapping_snapshot,
        &key_policy
            .read()
            .map_err(|_| WorkerError::KeyPolicyStatePoisoned)?
            .clone(),
        input,
        Instant::now(),
        Some((overlay, source)),
    )?;
    // RULE-SEC-002: Realtime Log is a view of events that were actually persisted.
    // DisplayOnly / Ignore / excluded / security-gated / duplicate inputs never enter it.
    if trace.outcome == ProcessOutcome::Recorded {
        if let Some(shortcut) = trace.shortcut.clone() {
            realtime_log.push(
                trace.occurred_at,
                shortcut,
                trace.application.clone(),
                trace.outcome.as_log_value(),
            );
        }
    }
    stats.record(trace.outcome);
    Ok(())
}

impl WorkerStats {
    fn record(&mut self, outcome: ProcessOutcome) {
        match outcome {
            ProcessOutcome::Recorded => self.recorded += 1,
            ProcessOutcome::DroppedIneligible => self.dropped_ineligible += 1,
            ProcessOutcome::DroppedUnsupported => self.dropped_unsupported += 1,
            ProcessOutcome::DroppedExcluded => self.dropped_excluded += 1,
            ProcessOutcome::DroppedDuplicate => self.dropped_duplicate += 1,
            ProcessOutcome::DroppedInvalidApplication => self.dropped_invalid_application += 1,
            ProcessOutcome::DroppedMapping => self.dropped_mapping += 1,
            ProcessOutcome::DisplayedOnly => self.displayed_only += 1,
            ProcessOutcome::DroppedIgnored => self.dropped_ignored += 1,
        }
    }
}

/// 1入力を仕様の順序で処理する。
pub fn process_captured_input(
    connection: &mut Connection,
    duplicate_guard: &mut DuplicateGuard,
    excluded_apps: &HashSet<String>,
    input: CapturedInput,
    now: Instant,
) -> Result<ProcessOutcome, WorkerError> {
    let mapping = KeyMapping::default();
    let policy = KeyPolicyConfig::default();
    Ok(process_captured_input_detailed(
        connection,
        duplicate_guard,
        excluded_apps,
        &mapping,
        &policy,
        input,
        now,
        None,
    )?
    .outcome)
}

#[derive(Clone, Debug)]
struct ProcessTrace {
    outcome: ProcessOutcome,
    occurred_at: chrono::NaiveDateTime,
    shortcut: Option<String>,
    application: String,
}

impl ProcessOutcome {
    fn as_log_value(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::DroppedIneligible => "ineligible",
            Self::DroppedUnsupported => "unsupported",
            Self::DroppedExcluded => "excluded",
            Self::DroppedDuplicate => "duplicate",
            Self::DroppedInvalidApplication => "invalid_application",
            Self::DroppedMapping => "mapping_disabled",
            Self::DisplayedOnly => "display_only",
            Self::DroppedIgnored => "ignored",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayCandidateError {
    Ineligible,
    Unsupported(u16),
}

/// Hook入力をDB記録より前のOverlay候補へ変換する公開ヘルパー。
/// 既存のworker経路ではsourceとworker受信時刻を明示して呼び出す。
pub fn build_overlay_candidate(input: &CapturedInput) -> Option<OverlayEvent> {
    build_overlay_candidate_for(input, OverlayCaptureSource::Local, Instant::now()).ok()
}

fn build_overlay_candidate_for(
    input: &CapturedInput,
    source: OverlayCaptureSource,
    now: Instant,
) -> Result<OverlayEvent, OverlayCandidateError> {
    if matches!(&input.main_input, MainInput::Key(vk) if is_modifier_key(*vk)) {
        return Err(OverlayCandidateError::Ineligible);
    }
    let has_modifier =
        input.modifiers.ctrl || input.modifiers.alt || input.modifiers.win || input.modifiers.shift;
    let kind = if has_modifier {
        OverlayInputKind::Shortcut
    } else {
        OverlayInputKind::SingleKey
    };

    // RULE-UI-011: modifier-only is rejected above; wheel-only is also intentionally
    // invisible. Modified wheel gestures remain normal shortcut candidates.
    if !has_modifier
        && matches!(
            input.main_input,
            MainInput::WheelUp
                | MainInput::WheelDown
                | MainInput::WheelLeft
                | MainInput::WheelRight
        )
    {
        return Err(OverlayCandidateError::Ineligible);
    }

    let (display_key, normalized_shortcut) = match kind {
        OverlayInputKind::Shortcut => {
            let shortcut = normalize_input(&input.modifiers, &input.main_input).map_err(
                |error| match error {
                    shortcut_core::normalizer::NormalizeError::UnsupportedKey(vk) => {
                        OverlayCandidateError::Unsupported(vk)
                    }
                    shortcut_core::normalizer::NormalizeError::IneligibleInput => {
                        OverlayCandidateError::Ineligible
                    }
                },
            )?;
            (shortcut.clone(), Some(shortcut))
        }
        OverlayInputKind::SingleKey => {
            let key = match input.main_input {
                MainInput::Key(vk) => normalize_single_key(u32::from(vk))
                    .ok_or(OverlayCandidateError::Unsupported(vk))?,
                MainInput::MouseLeft => "MouseLeft".to_owned(),
                MainInput::MouseRight => "MouseRight".to_owned(),
                MainInput::MouseMiddle => "MouseMiddle".to_owned(),
                MainInput::MouseX1 => "MouseX1".to_owned(),
                MainInput::MouseX2 => "MouseX2".to_owned(),
                MainInput::WheelUp => "WheelUp".to_owned(),
                MainInput::WheelDown => "WheelDown".to_owned(),
                MainInput::WheelLeft => "WheelLeft".to_owned(),
                MainInput::WheelRight => "WheelRight".to_owned(),
            };
            (key, None)
        }
    };

    Ok(OverlayEvent {
        display_key,
        normalized_shortcut,
        application: input.foreground_executable.clone(),
        source,
        occurred_at: input.occurred_at,
        received_at: now,
        kind,
        display_break: true,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_captured_input_detailed(
    connection: &mut Connection,
    duplicate_guard: &mut DuplicateGuard,
    excluded_apps: &HashSet<String>,
    key_mapping: &KeyMapping,
    key_policy: &KeyPolicyConfig,
    input: CapturedInput,
    now: Instant,
    overlay: Option<(&OverlayHandle, OverlayCaptureSource)>,
) -> Result<ProcessTrace, WorkerError> {
    let occurred_at = input.occurred_at;
    let application = input.foreground_executable.clone();

    let candidate = match build_overlay_candidate_for(&input, source_or_local(overlay), now) {
        Ok(candidate) => candidate,
        Err(OverlayCandidateError::Ineligible) => {
            return Ok(ProcessTrace {
                outcome: ProcessOutcome::DroppedIneligible,
                occurred_at,
                shortcut: None,
                application,
            })
        }
        Err(OverlayCandidateError::Unsupported(_)) => {
            return Ok(ProcessTrace {
                outcome: ProcessOutcome::DroppedUnsupported,
                occurred_at,
                shortcut: None,
                application,
            })
        }
    };

    if input.foreground_executable.trim().is_empty() {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DroppedInvalidApplication,
            occurred_at,
            shortcut: candidate.normalized_shortcut.clone(),
            application,
        });
    }
    if excluded_apps.contains(&input.foreground_executable.to_ascii_lowercase()) {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DroppedExcluded,
            occurred_at,
            shortcut: candidate.normalized_shortcut.clone(),
            application,
        });
    }

    let action = key_policy.resolve(candidate.identity(), candidate.kind);
    let mut decision = KeyDecision::from(action);
    // RULE-SEC-001 / RULE-SEC-006: no unmodified physical input may ever become
    // persistent data, even if a hand-edited policy JSON asks for recording.
    // Overlay display remains available for ordinary keyboard/mouse single inputs.
    if !has_any_modifier(&input.modifiers) {
        decision.record = false;
    }
    let mut candidate = candidate;
    candidate.display_break = decision.break_row;
    if decision.display {
        if let Some(overlay) = overlay.map(|(overlay, _)| overlay) {
            overlay.try_show(candidate.clone());
        }
    }

    // UI/JSONがどのactionを指定しても、修飾なし単独入力は上のSecurity invariantで
    // record=falseへ固定される。修飾付き入力だけがRecordAndDisplayBreakで記録可能。
    if action == KeyAction::Ignore {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DroppedIgnored,
            occurred_at,
            shortcut: None,
            application,
        });
    }
    if !decision.record {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DisplayedOnly,
            occurred_at,
            shortcut: Some(candidate.identity().to_owned()),
            application,
        });
    }

    // Overlay候補の生成・表示は既存キーマッピングより前に行う。キーマッピングで
    // DB記録を無効にしても、表示ポリシーが許可していればOverlayは表示する。
    if !key_mapping.allows(&input.main_input) {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DroppedMapping,
            occurred_at,
            shortcut: candidate.normalized_shortcut.clone(),
            application,
        });
    }

    let shortcut = candidate.identity().to_owned();
    if !duplicate_guard.should_accept(&shortcut, now) {
        return Ok(ProcessTrace {
            outcome: ProcessOutcome::DroppedDuplicate,
            occurred_at,
            shortcut: Some(shortcut),
            application,
        });
    }

    let event = RecordableEvent {
        occurred_at: input.occurred_at,
        shortcut: shortcut.clone(),
        application: input.foreground_executable,
    };
    record_event(connection, &event)?;
    duplicate_guard.mark_recorded(shortcut.clone(), now);
    Ok(ProcessTrace {
        outcome: ProcessOutcome::Recorded,
        occurred_at,
        shortcut: Some(shortcut),
        application: event.application,
    })
}

fn has_any_modifier(modifiers: &shortcut_core::model::ModifierState) -> bool {
    modifiers.ctrl || modifiers.alt || modifiers.win || modifiers.shift
}

fn source_or_local(
    overlay: Option<(&OverlayHandle, OverlayCaptureSource)>,
) -> OverlayCaptureSource {
    overlay
        .map(|(_, source)| source)
        .unwrap_or(OverlayCaptureSource::Local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rusqlite::Connection;
    use shortcut_core::model::{MainInput, ModifierState};
    use shortcut_storage::migration::run_migrations;
    use std::collections::HashMap;

    fn event(application: &str, input: MainInput) -> CapturedInput {
        CapturedInput {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("date")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("time"),
            modifiers: ModifierState {
                ctrl: true,
                alt: false,
                win: false,
                shift: false,
            },
            main_input: input,
            foreground_executable: application.to_owned(),
            pointer: None,
        }
    }

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory database");
        run_migrations(&mut connection).expect("migration");
        connection
    }

    #[test]
    fn single_letter_becomes_display_event_but_not_recordable_shortcut() {
        let mut input = event("Code.exe", MainInput::Key(0x41));
        input.modifiers = ModifierState::default();

        let candidate = build_overlay_candidate(&input).expect("single key candidate");

        assert_eq!(candidate.kind, OverlayInputKind::SingleKey);
        assert_eq!(candidate.display_key, "A");
        assert!(candidate.normalized_shortcut.is_none());
    }

    #[test]
    fn shift_compound_key_is_a_policy_selectable_shortcut() {
        let mut input = event("Code.exe", MainInput::Key(0x41));
        input.modifiers = ModifierState {
            shift: true,
            ..ModifierState::default()
        };
        let candidate = build_overlay_candidate(&input).expect("shift key candidate");
        assert_eq!(candidate.kind, OverlayInputKind::Shortcut);
        assert_eq!(candidate.normalized_shortcut.as_deref(), Some("Shift+A"));
    }

    #[test]
    fn single_key_display_only_does_not_add_a_database_event() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let mut input = event("Code.exe", MainInput::Key(0x41));
        input.modifiers = ModifierState::default();

        assert_eq!(
            process_captured_input(
                &mut connection,
                &mut guard,
                &excluded,
                input,
                Instant::now()
            )
            .unwrap(),
            ProcessOutcome::DisplayedOnly
        );
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 0);
    }

    #[test]
    fn single_key_record_policy_is_overridden_by_security_invariant() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let mut input = event("Code.exe", MainInput::Key(0x41));
        input.modifiers = ModifierState::default();
        let policy = KeyPolicyConfig {
            default_single_key: KeyAction::RecordAndDisplayBreak,
            ..KeyPolicyConfig::default()
        };

        let trace = process_captured_input_detailed(
            &mut connection,
            &mut guard,
            &excluded,
            &KeyMapping::default(),
            &policy,
            input,
            Instant::now(),
            None,
        )
        .expect("single key security decision");
        assert_eq!(trace.outcome, ProcessOutcome::DisplayedOnly);
        assert_eq!(trace.shortcut.as_deref(), Some("A"));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 0);
    }

    #[test]
    fn ignored_policy_suppresses_overlay_candidate_and_database_record() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let mut policy = KeyPolicyConfig::default();
        policy
            .overrides
            .insert("Ctrl+C".to_owned(), KeyAction::Ignore);

        let trace = process_captured_input_detailed(
            &mut connection,
            &mut guard,
            &excluded,
            &KeyMapping::default(),
            &policy,
            event("Code.exe", MainInput::Key(0x43)),
            Instant::now(),
            None,
        )
        .expect("policy decision");
        assert_eq!(trace.outcome, ProcessOutcome::DroppedIgnored);
        assert_eq!(trace.shortcut, None);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 0);
    }

    #[test]
    fn display_only_shortcut_policy_keeps_overlay_separate_from_database() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let policy = KeyPolicyConfig {
            overrides: HashMap::from([("Ctrl+C".to_owned(), KeyAction::ContinueDisplay)]),
            ..KeyPolicyConfig::default()
        };

        let trace = process_captured_input_detailed(
            &mut connection,
            &mut guard,
            &excluded,
            &KeyMapping::default(),
            &policy,
            event("Code.exe", MainInput::Key(0x43)),
            Instant::now(),
            None,
        )
        .expect("policy decision");
        assert_eq!(trace.outcome, ProcessOutcome::DisplayedOnly);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 0);
    }

    #[test]
    fn process_order_records_then_suppresses_cross_app_duplicate() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let now = Instant::now();

        assert_eq!(
            process_captured_input(
                &mut connection,
                &mut guard,
                &excluded,
                event("EXCEL.EXE", MainInput::Key(0x43)),
                now,
            )
            .unwrap(),
            ProcessOutcome::Recorded
        );
        assert_eq!(
            process_captured_input(
                &mut connection,
                &mut guard,
                &excluded,
                event("msedge.exe", MainInput::Key(0x43)),
                now + std::time::Duration::from_millis(500),
            )
            .unwrap(),
            ProcessOutcome::DroppedDuplicate
        );

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 1);
    }

    #[test]
    fn ineligible_and_excluded_inputs_never_reach_database() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let mut excluded = HashSet::new();
        excluded.insert("keepass.exe".to_owned());
        let now = Instant::now();

        let mut ordinary = event("Code.exe", MainInput::Key(0x41));
        ordinary.modifiers = ModifierState::default();
        assert_eq!(
            process_captured_input(&mut connection, &mut guard, &excluded, ordinary, now,).unwrap(),
            ProcessOutcome::DisplayedOnly
        );
        assert_eq!(
            process_captured_input(
                &mut connection,
                &mut guard,
                &excluded,
                event("KeePass.EXE", MainInput::Key(0x43)),
                now,
            )
            .unwrap(),
            ProcessOutcome::DroppedExcluded
        );

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(count, 0);
    }

    #[test]
    fn disabled_key_mapping_drops_before_recording() {
        let mut connection = connection();
        let mut guard = DuplicateGuard::new();
        let excluded = HashSet::new();
        let mut mapping = KeyMapping::default();
        mapping
            .keyboard
            .iter_mut()
            .find(|entry| entry.virtual_key == 0x43)
            .expect("C")
            .enabled = false;
        let trace = process_captured_input_detailed(
            &mut connection,
            &mut guard,
            &excluded,
            &mapping,
            &KeyPolicyConfig::default(),
            event("Code.exe", MainInput::Key(0x43)),
            Instant::now(),
            None,
        )
        .expect("mapping decision");
        assert_eq!(trace.outcome, ProcessOutcome::DroppedMapping);
        assert_eq!(trace.shortcut.as_deref(), Some("Ctrl+C"));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM shortcut_events", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn queue_overflow_is_non_blocking_and_counted() {
        let queue = CaptureQueue::with_capacity(1);
        let first = event("Code.exe", MainInput::Key(0x43));
        let second = event("Code.exe", MainInput::Key(0x44));

        queue.try_send(first).expect("first send");
        assert!(matches!(queue.try_send(second), Err(TrySendError::Full(_))));
        assert_eq!(queue.dropped_count(), 1);
    }

    #[test]
    fn worker_can_stop_without_ui() {
        let queue = CaptureQueue::new();
        let (_overlay_runtime, overlay) = OverlayRuntime::disabled(std::path::Path::new("."));
        let handle = start_capture_worker(
            queue.receiver(),
            queue.elevated_receiver(),
            connection(),
            shared_excluded_apps(HashSet::new()),
            new_capture_gate(),
            crate::key_mapping_state::shared(KeyMapping::default()),
            crate::key_policy::shared(KeyPolicyConfig::default()),
            RealtimeLog::default(),
            overlay,
            shared_duplicate_guard_threshold(1.0),
            crate::security_gate::shared_security_status(),
            crate::key_search::KeySearchCapture::default(),
        )
        .expect("worker start");
        let stats = handle.stop().expect("worker stop");
        assert_eq!(stats, WorkerStats::default());
    }
}
