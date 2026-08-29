use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use resvg::tiny_skia;
use shortcut_common::overlay::OverlayPosition;
use shortcut_core::model::{CapturedInput, MainInput};
use shortcut_windows::overlay_window::{
    calculate_overlay_origin, OverlayWindow, Point, Size, WorkArea,
};
use thiserror::Error;

use super::animation::{
    enter_transform, exit_transform, phase_at, progress, OverlayPhase, OverlayTiming,
};
use super::config::{
    initialize_overlay_files, load_labels, load_theme, save_theme_atomic, OverlayConfig,
    OverlayConfigError, OverlayDisplayMode, OverlayMonitorMode, OverlayPaths, OverlayPositionMode,
};
use super::event::{OverlayCaptureSource, OverlayEvent, OverlayInputKind};
use super::renderer::{OverlayRenderError, OverlayRenderer};

const OVERLAY_QUEUE_CAPACITY: usize = 64;
const SOURCE_DEDUP_WINDOW: Duration = Duration::from_millis(50);
const ANIMATION_FRAME: Duration = Duration::from_millis(16);
const REPEAT_BUMP_DURATION: Duration = Duration::from_millis(220);
pub const SINGLE_KEY_SEQUENCE_GAP: Duration = Duration::from_millis(700);
pub const OVERLAY_REPEAT_GAP: Duration = Duration::from_millis(700);
pub const MAX_SINGLE_KEY_TOKENS: usize = 20;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("overlay files could not be initialized: {0}")]
    Config(#[from] OverlayConfigError),
    #[error("overlay thread could not be started: {0}")]
    ThreadStart(#[source] std::io::Error),
}

#[derive(Clone)]
pub struct OverlayHandle {
    sender: Sender<OverlayCommand>,
    enabled: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    paths: Arc<OverlayPaths>,
    bounds: Arc<RwLock<Option<OverlayBounds>>>,
}

impl OverlayHandle {
    pub fn try_show(&self, event: OverlayEvent) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        if matches!(
            self.sender.try_send(OverlayCommand::Show(event)),
            Err(TrySendError::Full(_))
        ) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        match self.sender.try_send(OverlayCommand::SetEnabled(enabled)) {
            Err(TrySendError::Full(command)) => {
                let _ = self.sender.send(command);
            }
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn reload_assets(&self) {
        match self.sender.try_send(OverlayCommand::ReloadAssets) {
            Err(TrySendError::Full(command)) => {
                let _ = self.sender.send(command);
            }
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Security Gate遷移時に表示sessionを破棄し、repeat runを次入力へ持ち越さない。
    pub fn reset_session(&self) {
        match self.sender.try_send(OverlayCommand::ResetSession) {
            Err(TrySendError::Full(command)) => {
                let _ = self.sender.send(command);
            }
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn settings_snapshot(&self) -> OverlayConfig {
        load_theme(&self.paths.theme_json)
    }

    pub fn save_settings(
        &self,
        config: &OverlayConfig,
    ) -> Result<OverlayConfig, OverlayConfigError> {
        let config = config.clone().sanitize();
        save_theme_atomic(&self.paths.theme_json, &config)?;
        self.reload_assets();
        Ok(config)
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Overlay設定保存後のSAMPLEは実入力経路を通さずrendererへ直接渡す。
    pub fn preview_sample(&self) {
        let _ = self.sender.try_send(OverlayCommand::PreviewSample);
    }

    /// 保存前の外観draftをPreview専用経路へ渡す。generationはUI側の入力順を表し、
    /// Overlay threadは古いdraftを破棄する。
    pub fn preview_draft(&self, config: OverlayConfig, generation: u64) {
        let _ = self
            .sender
            .try_send(OverlayCommand::PreviewDraft { config, generation });
    }

    /// 固定モニター選択直後の識別プレビュー。実入力経路・DB・Realtime Logは通さない。
    pub fn preview_monitor(&self, monitor_id: String, label: String) {
        let _ = self
            .sender
            .try_send(OverlayCommand::PreviewMonitor { monitor_id, label });
    }

    /// Click-through Overlayの退避操作。元クリックはHookでブロックせず、
    /// この関数はShortcutKeyCounter側の表示/記録経路だけを消費する。
    pub fn consume_reposition_click(&self, input: &CapturedInput) -> bool {
        if input.main_input != MainInput::MouseLeft {
            return false;
        }
        let Some(point) = input.pointer else {
            return false;
        };
        let inside = self
            .bounds
            .read()
            .ok()
            .and_then(|v| *v)
            .is_some_and(|b| b.contains(point.x, point.y));
        if inside {
            let _ = self.sender.try_send(OverlayCommand::MoveOpposite);
            return true;
        }
        false
    }

    pub fn labels_path(&self) -> &Path {
        &self.paths.labels_json
    }

    pub fn theme_path(&self) -> &Path {
        &self.paths.theme_json
    }

    pub fn policy_path(&self) -> &Path {
        &self.paths.policy_json
    }

    pub fn design_directory(&self) -> &Path {
        &self.paths.design_directory
    }
}

pub struct OverlayRuntime {
    sender: Sender<OverlayCommand>,
    join_handle: Option<JoinHandle<()>>,
}

impl OverlayRuntime {
    pub fn disabled(app_dir: &Path) -> (Self, OverlayHandle) {
        let (sender, receiver) = bounded(1);
        drop(receiver);
        let paths = Arc::new(OverlayPaths {
            theme_json: app_dir.join("overlay-theme.json"),
            labels_json: app_dir.join("overlay_key_labels.json"),
            policy_json: app_dir.join("overlay_key_policy.json"),
            design_directory: app_dir.join("overlay/default"),
        });
        let handle = OverlayHandle {
            sender: sender.clone(),
            enabled: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU64::new(0)),
            paths,
            bounds: Arc::new(RwLock::new(None)),
        };
        (
            Self {
                sender,
                join_handle: None,
            },
            handle,
        )
    }

    pub fn start(app_dir: &Path, enabled: bool) -> Result<(Self, OverlayHandle), OverlayError> {
        let paths = Arc::new(initialize_overlay_files(app_dir)?);
        let (sender, receiver) = bounded(OVERLAY_QUEUE_CAPACITY);
        let enabled_state = Arc::new(AtomicBool::new(enabled));
        let dropped = Arc::new(AtomicU64::new(0));
        let bounds = Arc::new(RwLock::new(None));
        let thread_bounds = Arc::clone(&bounds);
        let thread_paths = Arc::clone(&paths);
        let thread_enabled = Arc::clone(&enabled_state);
        let join_handle = thread::Builder::new()
            .name("shortcut-overlay".to_owned())
            .spawn(move || {
                run_overlay_thread(receiver, thread_paths, thread_enabled, thread_bounds)
            })
            .map_err(OverlayError::ThreadStart)?;

        let handle = OverlayHandle {
            sender: sender.clone(),
            enabled: enabled_state,
            dropped,
            paths,
            bounds,
        };
        Ok((
            Self {
                sender,
                join_handle: Some(join_handle),
            },
            handle,
        ))
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.sender.send(OverlayCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for OverlayRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
enum OverlayCommand {
    Show(OverlayEvent),
    SetEnabled(bool),
    ReloadAssets,
    ResetSession,
    PreviewSample,
    PreviewDraft {
        config: OverlayConfig,
        generation: u64,
    },
    PreviewMonitor {
        monitor_id: String,
        label: String,
    },
    MoveOpposite,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OverlayBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl OverlayBounds {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayRowKind {
    Shortcut,
    SingleKeySequence,
}

#[derive(Debug)]
pub struct OverlayItem {
    pub id: u64,
    pub kind: OverlayRowKind,
    pub tokens: Vec<String>,
    pub created_at: Instant,
    pub last_input_at: Instant,
    pub last_token_started_at: Instant,
    pub phase: OverlayPhase,
    /// v0.1.4 repeat compressor state. identity is normalized internal identity, never display label.
    pub repeat_identity: Option<String>,
    pub repeat_count: u32,
    pub repeat_start_index: usize,
    pub repeat_token_len: usize,
    pub repeat_bump_started_at: Option<Instant>,
    /// Preview-only items are allowed to render even while normal Overlay is disabled.
    preview_only: bool,
    /// Monitor identification preview can override the normal target without mutating settings.
    monitor_override: Option<String>,
    position_override: Option<OverlayPosition>,
    move_started: Option<Instant>,
    move_rows: f32,
}

#[derive(Clone, Debug)]
struct RecentSourceEvent {
    identity: String,
    application: String,
    source: OverlayCaptureSource,
    captured_at: chrono::NaiveDateTime,
    received_at: Instant,
}

fn preview_sample_item(id: u64, now: Instant) -> OverlayItem {
    OverlayItem {
        id,
        kind: OverlayRowKind::SingleKeySequence,
        tokens: ["S", "A", "M", "P", "L", "E"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        created_at: now,
        last_input_at: now,
        last_token_started_at: now,
        phase: OverlayPhase::Stable,
        repeat_identity: None,
        repeat_count: 1,
        repeat_start_index: 0,
        repeat_token_len: 1,
        repeat_bump_started_at: None,
        preview_only: true,
        monitor_override: None,
        position_override: None,
        move_started: None,
        move_rows: 0.0,
    }
}

fn restore_preview_config(
    renderer: &mut OverlayRenderer,
    paths: &OverlayPaths,
    preview_base_config: &mut Option<OverlayConfig>,
) {
    let Some(config) = preview_base_config.clone() else {
        return;
    };
    let labels = load_labels(&paths.labels_json);
    if renderer.replace_assets(paths, config, labels).is_ok() {
        *preview_base_config = None;
    }
}

fn accept_preview_generation(latest: &mut u64, generation: u64) -> bool {
    if generation < *latest {
        return false;
    }
    *latest = generation;
    true
}

fn run_overlay_thread(
    receiver: Receiver<OverlayCommand>,
    paths: Arc<OverlayPaths>,
    enabled_state: Arc<AtomicBool>,
    bounds: Arc<RwLock<Option<OverlayBounds>>>,
) {
    let config = load_theme(&paths.theme_json);
    let labels = load_labels(&paths.labels_json);
    let Ok(mut renderer) = OverlayRenderer::load(&paths, config, labels) else {
        enabled_state.store(false, Ordering::Release);
        return;
    };
    let Ok(mut window) = OverlayWindow::create() else {
        enabled_state.store(false, Ordering::Release);
        return;
    };

    let mut items: VecDeque<OverlayItem> = VecDeque::new();
    let mut recent_sources: VecDeque<RecentSourceEvent> = VecDeque::new();
    let mut last_position: Option<OverlayPosition> = None;
    let mut next_id = 1u64;
    let mut enabled = enabled_state.load(Ordering::Acquire);
    // 保存前draftの描画中だけ、永続設定へ戻すための基準値を保持する。
    // draftはOverlay thread内のPreview item専用で、Capture/DB側の状態には触れない。
    let mut preview_base_config: Option<OverlayConfig> = None;
    let mut latest_preview_generation = 0u64;

    loop {
        window.pump_messages();
        let requested_enabled = enabled_state.load(Ordering::Acquire);
        if requested_enabled != enabled {
            enabled = requested_enabled;
            if !enabled {
                items.clear();
                recent_sources.clear();
                if let Ok(mut value) = bounds.write() {
                    *value = None;
                }
                window.hide();
            }
        }

        let now = Instant::now();
        remove_expired(&mut items, now, renderer.config());
        if preview_base_config.is_some() && !items.iter().any(|item| item.preview_only) {
            restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
        }
        if items.is_empty() {
            if let Ok(mut value) = bounds.write() {
                *value = None;
            }
        }
        prune_source_dedup(&mut recent_sources, now);

        let has_preview = items.back().is_some_and(|item| item.preview_only);
        if (enabled || has_preview) && !items.is_empty() {
            if render_frame(
                &mut window,
                &mut renderer,
                &mut items,
                now,
                &bounds,
                &mut last_position,
            )
            .is_err()
            {
                window.hide();
                enabled = false;
                enabled_state.store(false, Ordering::Release);
                items.clear();
                if let Ok(mut value) = bounds.write() {
                    *value = None;
                }
            }
        } else {
            window.hide();
        }

        let wait = next_wait_duration(&items, now, renderer.config());
        let command = if items.is_empty() {
            match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        } else {
            match receiver.recv_timeout(wait) {
                Ok(command) => Some(command),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        };

        let Some(command) = command else { continue };
        match command {
            OverlayCommand::Show(event) => {
                if !enabled || is_duplicate_source_event(&recent_sources, &event) {
                    continue;
                }
                if preview_base_config.is_some() {
                    restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                    items.clear();
                }
                recent_sources.push_back(RecentSourceEvent {
                    identity: event.identity().to_owned(),
                    application: event.application.clone(),
                    source: event.source,
                    captured_at: event.occurred_at,
                    received_at: event.received_at,
                });
                let config = renderer.config().clone();
                push_item(&mut items, event, Instant::now(), &config, &mut next_id);
            }
            OverlayCommand::SetEnabled(value) => {
                enabled = value;
                enabled_state.store(value, Ordering::Release);
                if !value {
                    restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                    items.clear();
                    recent_sources.clear();
                    if let Ok(mut value) = bounds.write() {
                        *value = None;
                    }
                    window.hide();
                }
            }
            OverlayCommand::ResetSession => {
                restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                items.clear();
                recent_sources.clear();
                if let Ok(mut value) = bounds.write() {
                    *value = None;
                }
                window.hide();
            }
            OverlayCommand::ReloadAssets => {
                restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                let config = load_theme(&paths.theme_json);
                let labels = load_labels(&paths.labels_json);
                if renderer.replace_assets(&paths, config, labels).is_ok() {
                    items.retain(|item| !item.preview_only);
                    enforce_max_rows(&mut items, renderer.config().max_rows);
                    if items.is_empty() {
                        window.hide();
                    }
                }
            }
            OverlayCommand::PreviewSample => {
                restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                items.clear();
                let now = Instant::now();
                items.push_back(preview_sample_item(next_id, now));
                next_id = next_id.wrapping_add(1);
            }
            OverlayCommand::PreviewDraft { config, generation } => {
                if !accept_preview_generation(&mut latest_preview_generation, generation) {
                    continue;
                }
                if preview_base_config.is_none() {
                    preview_base_config = Some(renderer.config().clone());
                }
                let labels = load_labels(&paths.labels_json);
                if renderer
                    .replace_assets(&paths, config.sanitize(), labels)
                    .is_err()
                {
                    restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                    items.clear();
                    if let Ok(mut value) = bounds.write() {
                        *value = None;
                    }
                    window.hide();
                    continue;
                }
                items.clear();
                let now = Instant::now();
                items.push_back(preview_sample_item(next_id, now));
                next_id = next_id.wrapping_add(1);
            }
            OverlayCommand::PreviewMonitor { monitor_id, label } => {
                restore_preview_config(&mut renderer, &paths, &mut preview_base_config);
                items.clear();
                let now = Instant::now();
                items.push_back(OverlayItem {
                    id: next_id,
                    kind: OverlayRowKind::Shortcut,
                    tokens: vec![label],
                    created_at: now,
                    last_input_at: now,
                    last_token_started_at: now,
                    phase: OverlayPhase::Stable,
                    repeat_identity: None,
                    repeat_count: 1,
                    repeat_start_index: 0,
                    repeat_token_len: 1,
                    repeat_bump_started_at: None,
                    preview_only: true,
                    monitor_override: Some(monitor_id),
                    position_override: Some(OverlayPosition::Center),
                    move_started: None,
                    move_rows: 0.0,
                });
                next_id = next_id.wrapping_add(1);
            }
            OverlayCommand::MoveOpposite => {
                if let Some(current) = last_position {
                    let mut config = load_theme(&paths.theme_json);
                    let target = opposite_position(current);
                    match config.position_mode {
                        OverlayPositionMode::Fixed => config.fixed_position = target,
                        OverlayPositionMode::Auto => {
                            config.preferred_position = target;
                            if !config.allowed_positions.contains(&target) {
                                config.allowed_positions.push(target);
                            }
                        }
                    }
                    if save_theme_atomic(&paths.theme_json, &config).is_ok() {
                        let labels = load_labels(&paths.labels_json);
                        let _ = renderer.replace_assets(&paths, config, labels);
                    }
                }
            }
            OverlayCommand::Stop => break,
        }
    }
    window.hide();
}

fn push_item(
    items: &mut VecDeque<OverlayItem>,
    event: OverlayEvent,
    now: Instant,
    config: &super::config::OverlayConfig,
    next_id: &mut u64,
) {
    let identity = event.identity().to_owned();

    // v0.1.4: Overlay repeat compressor is intentionally independent from DB DuplicateGuard.
    // Same normalized identity within 700ms belongs to one visual run.
    if let Some(last) = items.back_mut() {
        let same_run = last.repeat_identity.as_deref() == Some(identity.as_str())
            && now.saturating_duration_since(last.last_input_at) <= OVERLAY_REPEAT_GAP
            && !last.preview_only;
        if same_run {
            let shortcut_repeat = event.display_break
                && event.kind == OverlayInputKind::Shortcut
                && last.kind == OverlayRowKind::Shortcut;
            let continuation_repeat =
                !event.display_break && last.kind == OverlayRowKind::SingleKeySequence;
            if shortcut_repeat || continuation_repeat {
                last.repeat_count = last.repeat_count.saturating_add(1);
                if continuation_repeat {
                    if last.repeat_count == 2 {
                        last.tokens.push(event.display_key.clone());
                    } else if last.repeat_count == 3 {
                        let keep = last
                            .repeat_start_index
                            .saturating_add(last.repeat_token_len)
                            .min(last.tokens.len());
                        last.tokens.truncate(keep);
                    }
                }
                last.last_input_at = now;
                last.last_token_started_at = now;
                last.repeat_bump_started_at = (last.repeat_count >= 3).then_some(now);
                last.phase = OverlayPhase::Stable;
                return;
            }
        }
    }

    let continuation_tokens = vec![event.display_key.clone()];
    if !event.display_break
        && append_or_create_continuation(items, continuation_tokens, now, *next_id)
    {
        if let Some(last) = items.back_mut() {
            last.repeat_identity = Some(identity);
            last.repeat_count = 1;
            last.repeat_bump_started_at = None;
        }
        if items.len() == 1 && items.back().is_some_and(|item| item.id == *next_id) {
            *next_id = next_id.wrapping_add(1);
        }
        return;
    }

    for item in items.iter_mut() {
        item.move_rows += 1.0;
        item.move_started = Some(now);
    }
    let tokens = if event.kind == OverlayInputKind::SingleKey {
        vec![event.display_key]
    } else {
        event
            .display_key
            .split('+')
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let token_len = tokens.len();
    items.push_back(OverlayItem {
        id: *next_id,
        kind: if event.kind == OverlayInputKind::SingleKey {
            OverlayRowKind::SingleKeySequence
        } else {
            OverlayRowKind::Shortcut
        },
        tokens,
        created_at: now,
        last_input_at: now,
        last_token_started_at: now,
        phase: OverlayPhase::Entering,
        repeat_identity: Some(identity),
        repeat_count: 1,
        repeat_start_index: 0,
        repeat_token_len: token_len,
        repeat_bump_started_at: None,
        preview_only: false,
        monitor_override: None,
        position_override: None,
        move_started: None,
        move_rows: 0.0,
    });
    *next_id = next_id.wrapping_add(1);
    enforce_max_rows(items, config.max_rows);
}

fn append_or_create_continuation(
    items: &mut VecDeque<OverlayItem>,
    tokens: Vec<String>,
    now: Instant,
    next_id: u64,
) -> bool {
    if let Some(last) = items.back_mut() {
        let can_append = last.kind == OverlayRowKind::SingleKeySequence
            && now.saturating_duration_since(last.last_input_at) <= SINGLE_KEY_SEQUENCE_GAP
            && last.tokens.len().saturating_add(tokens.len()) <= MAX_SINGLE_KEY_TOKENS;
        if can_append {
            let repeat_start_index = last.tokens.len();
            let repeat_token_len = tokens.len();
            let repeat_identity = tokens.join("+");
            last.tokens.extend(tokens);
            last.repeat_identity = Some(repeat_identity);
            last.repeat_count = 1;
            last.repeat_start_index = repeat_start_index;
            last.repeat_token_len = repeat_token_len;
            last.repeat_bump_started_at = None;
            last.last_input_at = now;
            last.last_token_started_at = now;
            last.phase = OverlayPhase::Stable;
            return true;
        }
    }

    if items.is_empty() {
        let repeat_identity = tokens.join("+");
        let repeat_token_len = tokens.len();
        items.push_back(OverlayItem {
            id: next_id,
            kind: OverlayRowKind::SingleKeySequence,
            tokens,
            created_at: now,
            last_input_at: now,
            last_token_started_at: now,
            phase: OverlayPhase::Entering,
            repeat_identity: Some(repeat_identity),
            repeat_count: 1,
            repeat_start_index: 0,
            repeat_token_len,
            repeat_bump_started_at: None,
            preview_only: false,
            monitor_override: None,
            position_override: None,
            move_started: None,
            move_rows: 0.0,
        });
        return true;
    }
    false
}

/// 連続単独キーを既存の同一行へ追加する。追加できた場合はtrueを返す。
pub fn append_or_create_single_key(
    items: &mut VecDeque<OverlayItem>,
    key: String,
    now: Instant,
    next_id: u64,
) -> bool {
    append_or_create_continuation(items, vec![key], now, next_id)
}

pub fn enforce_max_rows(items: &mut VecDeque<OverlayItem>, max_rows: usize) {
    let max_rows = max_rows.clamp(1, 10);
    while items.len() > max_rows {
        items.pop_front();
    }
}

fn item_timing_origin(item: &OverlayItem) -> Instant {
    item.last_input_at
}

fn remove_expired(
    items: &mut VecDeque<OverlayItem>,
    now: Instant,
    config: &super::config::OverlayConfig,
) {
    let timing = OverlayTiming::from_config(config);
    while items
        .front()
        .is_some_and(|item| phase_at(now, item_timing_origin(item), &timing).is_none())
    {
        items.pop_front();
    }
}

fn prune_source_dedup(recent: &mut VecDeque<RecentSourceEvent>, now: Instant) {
    while recent
        .front()
        .is_some_and(|entry| now.saturating_duration_since(entry.received_at) > SOURCE_DEDUP_WINDOW)
    {
        recent.pop_front();
    }
}

fn is_duplicate_source_event(recent: &VecDeque<RecentSourceEvent>, event: &OverlayEvent) -> bool {
    recent.iter().rev().any(|entry| {
        entry.source != event.source
            && entry.identity == event.identity()
            && entry.application.eq_ignore_ascii_case(&event.application)
            && event
                .occurred_at
                .signed_duration_since(entry.captured_at)
                .num_milliseconds()
                .abs()
                <= SOURCE_DEDUP_WINDOW.as_millis() as i64
    })
}

fn render_frame(
    window: &mut OverlayWindow,
    renderer: &mut OverlayRenderer,
    items: &mut VecDeque<OverlayItem>,
    now: Instant,
    bounds: &Arc<RwLock<Option<OverlayBounds>>>,
    last_position: &mut Option<OverlayPosition>,
) -> Result<(), OverlayRenderError> {
    let config = renderer.config().clone();
    let monitor_override = items
        .back()
        .and_then(|item| item.monitor_override.as_deref());
    let (target_window, monitor, desktop_mode) = if let Some(monitor_id) = monitor_override {
        let metrics = window
            .fixed_monitor_metrics(monitor_id)
            .map_err(|_| OverlayRenderError::PixmapAllocation)?;
        (None, metrics, true)
    } else {
        match config.display_mode {
            OverlayDisplayMode::ActiveWindow => window
                .active_window_metrics()
                .map(|(target, metrics)| (Some(target), metrics, false))
                .map_err(|_| OverlayRenderError::PixmapAllocation)?,
            OverlayDisplayMode::Desktop => {
                let metrics = if config.monitor_mode == OverlayMonitorMode::Fixed
                    && !config.fixed_monitor_id.is_empty()
                {
                    window.fixed_monitor_metrics(&config.fixed_monitor_id)
                } else {
                    window.active_monitor_metrics()
                }
                .map_err(|_| OverlayRenderError::PixmapAllocation)?;
                (None, metrics, true)
            }
        }
    };
    window
        .set_display_target(target_window, desktop_mode)
        .map_err(|_| OverlayRenderError::PixmapAllocation)?;

    let scale = monitor.scale();
    let gap = (config.item_gap_px.max(0) as f32 * scale).round() as u32;
    let padding = (8.0 * scale).round().max(1.0) as u32;
    let animation_margin = (config.animation.slide_distance.abs() * scale).ceil() as u32 + 4;
    let margin = (config.margin_px.max(0) as f32 * scale).round() as i32;
    let work = WorkArea {
        left: monitor.work_left,
        top: monitor.work_top,
        right: monitor.work_right,
        bottom: monitor.work_bottom,
    };
    let work_width = (work.right - work.left).max(1) as u32;
    let reserved = (margin.max(0) as u32)
        .saturating_mul(2)
        .saturating_add(padding.saturating_mul(2))
        .saturating_add(animation_margin.saturating_mul(2));
    let max_content_width = work_width.saturating_sub(reserved).max(64);

    let mut rendered = Vec::with_capacity(items.len());
    let mut row_width = 1u32;
    let mut stack_height = 0u32;
    for item in items.iter() {
        let bitmap = render_item_bitmap(renderer, item, max_content_width, scale, now, &config)?;
        row_width = row_width.max(bitmap.width());
        stack_height = stack_height.saturating_add(bitmap.height());
        rendered.push(bitmap);
    }
    if rendered.len() > 1 {
        stack_height = stack_height.saturating_add(gap.saturating_mul(rendered.len() as u32 - 1));
    }
    let width = row_width
        .saturating_add(padding.saturating_mul(2))
        .saturating_add(animation_margin.saturating_mul(2))
        .min(work_width)
        .max(1);
    let max_work_height = (work.bottom - work.top).max(1) as u32;
    let height = stack_height
        .saturating_add(padding.saturating_mul(2))
        .saturating_add(animation_margin.saturating_mul(2))
        .min(max_work_height)
        .max(1);
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).ok_or(OverlayRenderError::PixmapAllocation)?;

    if let Some(path) = rounded_rect_path(width as f32, height as f32, 10.0 * scale) {
        let background = renderer.background_rgba();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba8(
            background.r,
            background.g,
            background.b,
            background.a,
        ));
        canvas.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let timing = OverlayTiming::from_config(&config);
    let mut cursor_y = animation_margin as f32 + padding as f32;
    for (item, bitmap) in items.iter_mut().zip(rendered.iter()) {
        let origin = item_timing_origin(item);
        item.phase = phase_at(now, origin, &timing).unwrap_or(OverlayPhase::Fading);
        // A single-key sequence never restarts animation for tokens already on screen.
        // Its newly appended token is animated inside render_wrapped_sequence instead.
        let transform = match (item.kind, item.phase) {
            (OverlayRowKind::SingleKeySequence, OverlayPhase::Entering | OverlayPhase::Stable) => {
                super::animation::Transform::identity()
            }
            (OverlayRowKind::Shortcut, OverlayPhase::Entering) if item.repeat_count > 1 => {
                // Repeat input extends visibility but must not restart the keycap IN animation.
                super::animation::Transform::identity()
            }
            (_, OverlayPhase::Entering) => enter_transform(
                config.animation.enter,
                progress(now, origin, config.animation.enter_ms),
                config.animation.slide_distance,
                config.animation.start_scale,
            ),
            (_, OverlayPhase::Stable) => super::animation::Transform::identity(),
            (_, OverlayPhase::Fading) => exit_transform(
                config.animation.exit,
                progress(now, timing.exit_at(origin), config.animation.exit_ms),
                config.animation.slide_distance,
            ),
        };
        let move_offset = if let Some(started) = item.move_started {
            let move_progress = progress(now, started, config.animation.enter_ms);
            let remaining = 1.0 - ease_out_cubic(move_progress);
            if move_progress >= 1.0 {
                item.move_started = None;
                item.move_rows = 0.0;
                0.0
            } else {
                item.move_rows * (bitmap.height().saturating_add(gap)) as f32 * remaining
            }
        } else {
            0.0
        };
        let transformed_width = bitmap.width() as f32 * transform.scale;
        let x =
            ((width as f32 - transformed_width) / 2.0 + transform.offset_x * scale).round() as i32;
        let y = (cursor_y + transform.offset_y * scale + move_offset).round() as i32;
        let paint = tiny_skia::PixmapPaint {
            opacity: transform.opacity.clamp(0.0, 1.0),
            quality: tiny_skia::FilterQuality::Bilinear,
            ..tiny_skia::PixmapPaint::default()
        };
        canvas.draw_pixmap(
            x,
            y,
            bitmap.as_ref().as_ref(),
            &paint,
            tiny_skia::Transform::from_scale(transform.scale, transform.scale),
            None,
        );
        cursor_y += bitmap.height() as f32 + gap as f32;
    }

    let position_override = items.back().and_then(|item| item.position_override);
    let size = Size {
        width: width as i32,
        height: height as i32,
    };
    let auto_position =
        position_override.is_none() && config.position_mode == OverlayPositionMode::Auto;
    let position = if let Some(position) = position_override {
        position
    } else {
        match config.position_mode {
            OverlayPositionMode::Fixed => config.fixed_position,
            OverlayPositionMode::Auto => {
                let window_rects = window
                    .visible_window_rects_on_monitor(work)
                    .unwrap_or_default();
                let focused_editor = window.focused_editable_rect().ok().flatten();
                let Some(position) = choose_auto_position(
                    &config,
                    work,
                    size,
                    &window_rects,
                    focused_editor,
                    margin,
                ) else {
                    window.hide();
                    if let Ok(mut value) = bounds.write() {
                        *value = None;
                    }
                    return Ok(());
                };
                position
            }
        }
    };
    let raw_origin = calculate_overlay_origin(work, size, position, margin);
    let origin = if auto_position {
        raw_origin
    } else {
        clamp_origin(work, size, raw_origin)
    };
    window
        .present_rgba(canvas.data(), width, height, origin.x, origin.y)
        .map_err(|_| OverlayRenderError::PixmapAllocation)?;
    *last_position = Some(position);
    if let Ok(mut value) = bounds.write() {
        *value = Some(OverlayBounds {
            left: origin.x,
            top: origin.y,
            right: origin.x.saturating_add(width as i32),
            bottom: origin.y.saturating_add(height as i32),
        });
    }
    Ok(())
}

fn render_item_bitmap(
    renderer: &mut OverlayRenderer,
    item: &OverlayItem,
    max_content_width: u32,
    scale: f32,
    now: Instant,
    config: &OverlayConfig,
) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
    let gap = (7.0 * scale).round().max(2.0) as u32;
    match item.kind {
        OverlayRowKind::Shortcut => {
            let base = renderer.render_row(&item.tokens, true, scale)?;
            if item.repeat_count == 2 {
                compose_two(&base, &base, gap, max_content_width)
            } else if item.repeat_count >= 3 {
                let badge = renderer.render_repeat_badge(item.repeat_count, scale)?;
                let badge = animate_repeat_badge(&badge, repeat_bump_scale(item, now))?;
                compose_two(&base, &badge, gap, max_content_width)
            } else {
                Ok(base)
            }
        }
        OverlayRowKind::SingleKeySequence => {
            let badge = if item.repeat_count >= 3 {
                Some(renderer.render_repeat_badge(item.repeat_count, scale)?)
            } else {
                None
            };
            let badge_reserve = badge
                .as_ref()
                .map(|value| value.width().saturating_mul(5) / 4 + gap)
                .unwrap_or(0);
            let sequence_width = max_content_width.saturating_sub(badge_reserve).max(64);
            let base = render_wrapped_sequence(renderer, item, sequence_width, scale, now, config)?;
            if let Some(badge) = badge {
                let badge = animate_repeat_badge(&badge, repeat_bump_scale(item, now))?;
                compose_two(&base, &badge, gap, max_content_width)
            } else {
                Ok(base)
            }
        }
    }
}

fn repeat_bump_scale(item: &OverlayItem, now: Instant) -> f32 {
    let Some(started) = item.repeat_bump_started_at else {
        return 1.0;
    };
    let elapsed = now.saturating_duration_since(started);
    if elapsed >= REPEAT_BUMP_DURATION {
        return 1.0;
    }
    let progress = elapsed.as_secs_f32() / REPEAT_BUMP_DURATION.as_secs_f32();
    if progress < 0.55 {
        let local = progress / 0.55;
        0.80 + (1.25 - 0.80) * ease_out_cubic(local)
    } else {
        let local = (progress - 0.55) / 0.45;
        1.25 + (1.0 - 1.25) * ease_out_cubic(local)
    }
}

fn animate_repeat_badge(
    badge: &Arc<tiny_skia::Pixmap>,
    scale: f32,
) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
    // Use a fixed 125% envelope so count-only animation does not move the key row.
    let width = ((badge.width() as f32 * 1.25).ceil() as u32).max(1);
    let height = ((badge.height() as f32 * 1.25).ceil() as u32).max(1);
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).ok_or(OverlayRenderError::PixmapAllocation)?;
    let scaled_width = badge.width() as f32 * scale;
    let scaled_height = badge.height() as f32 * scale;
    let x = ((width as f32 - scaled_width) / 2.0).round() as i32;
    let y = ((height as f32 - scaled_height) / 2.0).round() as i32;
    canvas.draw_pixmap(
        x,
        y,
        badge.as_ref().as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..tiny_skia::PixmapPaint::default()
        },
        tiny_skia::Transform::from_scale(scale, scale),
        None,
    );
    Ok(Arc::new(canvas))
}

fn compose_two(
    left: &Arc<tiny_skia::Pixmap>,
    right: &Arc<tiny_skia::Pixmap>,
    gap: u32,
    max_width: u32,
) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
    let horizontal_width = left
        .width()
        .saturating_add(gap)
        .saturating_add(right.width());
    let horizontal = horizontal_width <= max_width;
    let (width, height, left_x, left_y, right_x, right_y) = if horizontal {
        let height = left.height().max(right.height()).max(1);
        (
            horizontal_width.max(1),
            height,
            0,
            ((height - left.height()) / 2) as i32,
            left.width().saturating_add(gap) as i32,
            ((height - right.height()) / 2) as i32,
        )
    } else {
        let width = left.width().max(right.width()).min(max_width).max(1);
        (
            width,
            left.height()
                .saturating_add(gap)
                .saturating_add(right.height())
                .max(1),
            0,
            0,
            width.saturating_sub(right.width().min(width)) as i32,
            left.height().saturating_add(gap) as i32,
        )
    };
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).ok_or(OverlayRenderError::PixmapAllocation)?;
    canvas.draw_pixmap(
        left_x,
        left_y,
        left.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    canvas.draw_pixmap(
        right_x,
        right_y,
        right.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Ok(Arc::new(canvas))
}

fn render_wrapped_sequence(
    renderer: &mut OverlayRenderer,
    item: &OverlayItem,
    max_content_width: u32,
    scale: f32,
    now: Instant,
    config: &OverlayConfig,
) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
    let token_gap = (config.key.separator_gap as f32 * scale).round().max(2.0) as u32;
    let mut tokens = Vec::with_capacity(item.tokens.len());
    for token in &item.tokens {
        tokens.push(renderer.render_token(token, scale)?);
    }
    let key_height = tokens.iter().map(|p| p.height()).max().unwrap_or(1);
    let mut positions = Vec::with_capacity(tokens.len());
    let mut x = 0u32;
    let mut y = 0u32;
    let mut max_x = 1u32;
    for pixmap in &tokens {
        if x > 0 && x.saturating_add(pixmap.width()) > max_content_width {
            x = 0;
            y = y.saturating_add(key_height).saturating_add(token_gap);
        }
        positions.push((x, y));
        max_x = max_x.max(x.saturating_add(pixmap.width()));
        x = x.saturating_add(pixmap.width()).saturating_add(token_gap);
    }
    let height = y.saturating_add(key_height).max(1);
    let width = max_x.min(max_content_width.max(1)).max(1);
    let mut canvas =
        tiny_skia::Pixmap::new(width, height).ok_or(OverlayRenderError::PixmapAllocation)?;
    let last_index = tokens.len().saturating_sub(1);
    for (index, (pixmap, (x, y))) in tokens.iter().zip(positions.iter()).enumerate() {
        let transform = if index == last_index {
            enter_transform(
                config.animation.enter,
                progress(now, item.last_token_started_at, config.animation.enter_ms),
                config.animation.slide_distance.min(10.0),
                config.animation.start_scale,
            )
        } else {
            super::animation::Transform::identity()
        };
        let paint = tiny_skia::PixmapPaint {
            opacity: transform.opacity.clamp(0.0, 1.0),
            quality: tiny_skia::FilterQuality::Bilinear,
            ..tiny_skia::PixmapPaint::default()
        };
        canvas.draw_pixmap(
            (*x as f32 + transform.offset_x * scale).round() as i32,
            (*y as f32 + transform.offset_y * scale).round() as i32,
            pixmap.as_ref().as_ref(),
            &paint,
            tiny_skia::Transform::from_scale(transform.scale, transform.scale),
            None,
        );
    }
    Ok(Arc::new(canvas))
}

fn choose_auto_position(
    config: &OverlayConfig,
    work: WorkArea,
    size: Size,
    windows: &[WorkArea],
    focused_editor: Option<WorkArea>,
    margin: i32,
) -> Option<OverlayPosition> {
    let mut candidates = Vec::with_capacity(config.allowed_positions.len() + 1);
    candidates.push(config.preferred_position);
    for position in &config.allowed_positions {
        if !candidates.contains(position) {
            candidates.push(*position);
        }
    }

    let valid = candidates.into_iter().filter_map(|position| {
        let origin = calculate_overlay_origin(work, size, position, margin);
        let rect = WorkArea {
            left: origin.x,
            top: origin.y,
            right: origin.x.saturating_add(size.width),
            bottom: origin.y.saturating_add(size.height),
        };
        contains_rect(work, rect).then_some((position, rect))
    });
    let valid: Vec<_> = valid.collect();

    // 候補群A: 全ての表示Windowと重ならない候補。条件2の順位を混ぜない。
    if let Some((position, _)) = valid
        .iter()
        .find(|(_, rect)| windows.iter().all(|window| !overlaps(*rect, *window)))
    {
        return Some(*position);
    }

    // 候補群B: Aが空の場合だけfocused editorとの重複を除外する。
    if let Some((position, _)) = valid.iter().find(|(_, rect)| {
        focused_editor
            .map(|editor| !overlaps(*rect, editor))
            .unwrap_or(true)
    }) {
        return Some(*position);
    }

    // A/Bが空でもWork Area内の配置順位を維持する。
    valid.first().map(|(position, _)| *position)
}

fn overlaps(a: WorkArea, b: WorkArea) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn contains_rect(work: WorkArea, rect: WorkArea) -> bool {
    rect.left >= work.left
        && rect.top >= work.top
        && rect.right <= work.right
        && rect.bottom <= work.bottom
}

fn clamp_origin(work: WorkArea, size: Size, point: Point) -> Point {
    let max_x = (work.right - size.width).max(work.left);
    let max_y = (work.bottom - size.height).max(work.top);
    Point {
        x: point.x.clamp(work.left, max_x),
        y: point.y.clamp(work.top, max_y),
    }
}

fn opposite_position(position: OverlayPosition) -> OverlayPosition {
    match position {
        OverlayPosition::TopLeft => OverlayPosition::BottomRight,
        OverlayPosition::TopCenter => OverlayPosition::BottomCenter,
        OverlayPosition::TopRight => OverlayPosition::BottomLeft,
        OverlayPosition::MiddleLeft => OverlayPosition::MiddleRight,
        OverlayPosition::Center => OverlayPosition::BottomRight,
        OverlayPosition::MiddleRight => OverlayPosition::MiddleLeft,
        OverlayPosition::BottomLeft => OverlayPosition::TopRight,
        OverlayPosition::BottomCenter => OverlayPosition::TopCenter,
        OverlayPosition::BottomRight => OverlayPosition::TopLeft,
    }
}

fn next_wait_duration(
    items: &VecDeque<OverlayItem>,
    now: Instant,
    config: &super::config::OverlayConfig,
) -> Duration {
    if items.is_empty() {
        return Duration::from_secs(60);
    }
    let timing = OverlayTiming::from_config(config);
    let mut animating = false;
    let mut nearest = Duration::from_secs(1);
    for item in items {
        let origin = item_timing_origin(item);
        match phase_at(now, origin, &timing) {
            Some(OverlayPhase::Entering) | Some(OverlayPhase::Fading) => animating = true,
            Some(OverlayPhase::Stable) => {}
            None => nearest = Duration::ZERO,
        }
        let next = if now < timing.stable_at(origin) {
            timing.stable_at(origin).saturating_duration_since(now)
        } else if now < timing.exit_at(origin) {
            timing.exit_at(origin).saturating_duration_since(now)
        } else {
            timing.remove_at(origin).saturating_duration_since(now)
        };
        nearest = nearest.min(next);
        if item.move_started.is_some() {
            animating = true;
        }
        if let Some(started) = item.repeat_bump_started_at {
            let elapsed = now.saturating_duration_since(started);
            if elapsed < REPEAT_BUMP_DURATION {
                animating = true;
                nearest = nearest.min(REPEAT_BUMP_DURATION.saturating_sub(elapsed));
            }
        }
    }
    if animating {
        ANIMATION_FRAME.min(nearest.max(Duration::from_millis(1)))
    } else {
        nearest.max(Duration::from_millis(1))
    }
}

fn ease_out_cubic(value: f32) -> f32 {
    1.0 - (1.0 - value.clamp(0.0, 1.0)).powi(3)
}

fn rounded_rect_path(width: f32, height: f32, radius: f32) -> Option<tiny_skia::Path> {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let k = 0.552_284_8;
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(radius, 0.0);
    builder.line_to(width - radius, 0.0);
    builder.cubic_to(
        width - radius + radius * k,
        0.0,
        width,
        radius - radius * k,
        width,
        radius,
    );
    builder.line_to(width, height - radius);
    builder.cubic_to(
        width,
        height - radius + radius * k,
        width - radius + radius * k,
        height,
        width - radius,
        height,
    );
    builder.line_to(radius, height);
    builder.cubic_to(
        radius - radius * k,
        height,
        0.0,
        height - radius + radius * k,
        0.0,
        height - radius,
    );
    builder.line_to(0.0, radius);
    builder.cubic_to(
        0.0,
        radius - radius * k,
        radius - radius * k,
        0.0,
        radius,
        0.0,
    );
    builder.close();
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::config::OverlayConfig;

    fn captured_at() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 8, 16)
            .expect("date")
            .and_hms_milli_opt(11, 0, 0, 0)
            .expect("time")
    }

    fn event(kind: OverlayInputKind, key: &str, source: OverlayCaptureSource) -> OverlayEvent {
        OverlayEvent {
            display_key: key.to_owned(),
            normalized_shortcut: (kind == OverlayInputKind::Shortcut).then(|| key.to_owned()),
            application: "Code.exe".to_owned(),
            source,
            occurred_at: captured_at(),
            received_at: Instant::now(),
            kind,
            display_break: true,
        }
    }

    #[test]
    fn local_and_elevated_same_event_within_50ms_is_suppressed() {
        let now = Instant::now();
        let mut recent = VecDeque::new();
        recent.push_back(RecentSourceEvent {
            identity: "Ctrl+C".to_owned(),
            application: "Code.exe".to_owned(),
            source: OverlayCaptureSource::Local,
            captured_at: captured_at(),
            received_at: now,
        });
        let mut elevated = event(
            OverlayInputKind::Shortcut,
            "Ctrl+C",
            OverlayCaptureSource::Elevated,
        );
        elevated.application = "code.EXE".to_owned();
        elevated.occurred_at = captured_at() + chrono::Duration::milliseconds(20);
        assert!(is_duplicate_source_event(&recent, &elevated));
    }

    #[test]
    fn repeated_input_from_same_source_is_not_overlay_deduped() {
        let now = Instant::now();
        let mut recent = VecDeque::new();
        recent.push_back(RecentSourceEvent {
            identity: "Ctrl+C".to_owned(),
            application: "Code.exe".to_owned(),
            source: OverlayCaptureSource::Local,
            captured_at: captured_at(),
            received_at: now,
        });
        let local = event(
            OverlayInputKind::Shortcut,
            "Ctrl+C",
            OverlayCaptureSource::Local,
        );
        assert!(!is_duplicate_source_event(&recent, &local));
    }

    #[test]
    fn preview_generation_discards_only_older_drafts() {
        let mut latest = 4;
        assert!(!accept_preview_generation(&mut latest, 3));
        assert_eq!(latest, 4);
        assert!(accept_preview_generation(&mut latest, 5));
        assert_eq!(latest, 5);
        assert!(accept_preview_generation(&mut latest, 5));
        assert_eq!(latest, 5);
    }

    #[test]
    fn preview_item_is_not_part_of_a_repeat_run() {
        let item = preview_sample_item(1, Instant::now());
        assert!(item.preview_only);
        assert_eq!(item.repeat_identity, None);
        assert_eq!(item.repeat_count, 1);
        assert_eq!(item.tokens, vec!["S", "A", "M", "P", "L", "E"]);
    }

    #[test]
    fn consecutive_single_keys_append_to_same_row() {
        let t0 = Instant::now();
        let mut items = VecDeque::new();
        assert!(append_or_create_single_key(&mut items, "A".into(), t0, 1));
        assert!(append_or_create_single_key(
            &mut items,
            "B".into(),
            t0 + Duration::from_millis(100),
            2
        ));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tokens, vec!["A", "B"]);
    }

    #[test]
    fn shortcut_breaks_single_key_sequence() {
        let config = OverlayConfig::default();
        let mut items = VecDeque::new();
        let now = Instant::now();
        push_item(
            &mut items,
            event(
                OverlayInputKind::SingleKey,
                "A",
                OverlayCaptureSource::Local,
            ),
            now,
            &config,
            &mut 1,
        );
        push_item(
            &mut items,
            event(
                OverlayInputKind::Shortcut,
                "Ctrl+C",
                OverlayCaptureSource::Local,
            ),
            now + Duration::from_millis(100),
            &config,
            &mut 2,
        );
        push_item(
            &mut items,
            event(
                OverlayInputKind::SingleKey,
                "D",
                OverlayCaptureSource::Local,
            ),
            now + Duration::from_millis(200),
            &config,
            &mut 3,
        );
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].tokens, vec!["A"]);
        assert_eq!(items[1].tokens, vec!["Ctrl", "C"]);
        assert_eq!(items[2].tokens, vec!["D"]);
    }

    #[test]
    fn auto_position_prefers_non_overlapping_allowed_position() {
        let config = OverlayConfig {
            position_mode: OverlayPositionMode::Auto,
            allowed_positions: vec![OverlayPosition::Center, OverlayPosition::BottomRight],
            preferred_position: OverlayPosition::Center,
            ..OverlayConfig::default()
        };
        let selected = choose_auto_position(
            &config,
            WorkArea {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            Size {
                width: 400,
                height: 100,
            },
            &[WorkArea {
                left: 300,
                top: 200,
                right: 1600,
                bottom: 900,
            }],
            None,
            32,
        )
        .expect("a non-overlapping position");
        assert_eq!(selected, OverlayPosition::BottomRight);
    }

    #[test]
    fn auto_position_condition_a_beats_higher_ranked_condition_b() {
        let config = OverlayConfig {
            position_mode: OverlayPositionMode::Auto,
            allowed_positions: vec![OverlayPosition::TopLeft, OverlayPosition::BottomLeft],
            preferred_position: OverlayPosition::BottomRight,
            ..OverlayConfig::default()
        };
        let selected = choose_auto_position(
            &config,
            WorkArea {
                left: 0,
                top: 0,
                right: 1000,
                bottom: 800,
            },
            Size {
                width: 200,
                height: 100,
            },
            &[
                WorkArea {
                    left: 750,
                    top: 650,
                    right: 1000,
                    bottom: 800,
                },
                WorkArea {
                    left: 0,
                    top: 0,
                    right: 250,
                    bottom: 150,
                },
            ],
            Some(WorkArea {
                left: 400,
                top: 650,
                right: 650,
                bottom: 800,
            }),
            0,
        )
        .expect("a condition A position");
        assert_eq!(selected, OverlayPosition::BottomLeft);
    }

    #[test]
    fn auto_position_preserves_preferred_then_saved_order_deterministically() {
        let config = OverlayConfig {
            position_mode: OverlayPositionMode::Auto,
            allowed_positions: vec![OverlayPosition::TopRight, OverlayPosition::TopLeft],
            preferred_position: OverlayPosition::BottomRight,
            ..OverlayConfig::default()
        };
        let work = WorkArea {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        let windows = [WorkArea {
            left: 700,
            top: 650,
            right: 1000,
            bottom: 800,
        }];
        let first = choose_auto_position(
            &config,
            work,
            Size {
                width: 200,
                height: 100,
            },
            &windows,
            None,
            0,
        );
        let second = choose_auto_position(
            &config,
            work,
            Size {
                width: 200,
                height: 100,
            },
            &windows,
            None,
            0,
        );
        assert_eq!(first, Some(OverlayPosition::TopRight));
        assert_eq!(first, second);
    }

    #[test]
    fn auto_position_rejects_work_area_boundary_overflow() {
        let config = OverlayConfig {
            position_mode: OverlayPositionMode::Auto,
            allowed_positions: vec![OverlayPosition::Center],
            preferred_position: OverlayPosition::Center,
            ..OverlayConfig::default()
        };
        let selected = choose_auto_position(
            &config,
            WorkArea {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            Size {
                width: 101,
                height: 20,
            },
            &[],
            None,
            0,
        );
        assert_eq!(selected, None);
    }

    #[test]
    fn center_click_moves_to_bottom_right() {
        assert_eq!(
            opposite_position(OverlayPosition::Center),
            OverlayPosition::BottomRight
        );
    }

    #[test]
    fn shortcut_repeat_compresses_from_third_input() {
        let config = OverlayConfig::default();
        let now = Instant::now();
        let mut items = VecDeque::new();
        let mut next_id = 1;
        for offset in [0, 100, 200, 300] {
            push_item(
                &mut items,
                event(
                    OverlayInputKind::Shortcut,
                    "Ctrl+C",
                    OverlayCaptureSource::Local,
                ),
                now + Duration::from_millis(offset),
                &config,
                &mut next_id,
            );
        }
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tokens, vec!["Ctrl", "C"]);
        assert_eq!(items[0].repeat_count, 4);
    }

    #[test]
    fn single_key_repeat_keeps_two_tokens_then_compresses() {
        let config = OverlayConfig::default();
        let now = Instant::now();
        let mut items = VecDeque::new();
        let mut next_id = 1;
        for offset in [0, 100] {
            let mut input = event(
                OverlayInputKind::SingleKey,
                "A",
                OverlayCaptureSource::Local,
            );
            input.display_break = false;
            push_item(
                &mut items,
                input,
                now + Duration::from_millis(offset),
                &config,
                &mut next_id,
            );
        }
        assert_eq!(items[0].tokens, vec!["A", "A"]);
        let mut third = event(
            OverlayInputKind::SingleKey,
            "A",
            OverlayCaptureSource::Local,
        );
        third.display_break = false;
        push_item(
            &mut items,
            third,
            now + Duration::from_millis(200),
            &config,
            &mut next_id,
        );
        assert_eq!(items[0].tokens, vec!["A"]);
        assert_eq!(items[0].repeat_count, 3);
    }

    #[test]
    fn repeat_run_ends_after_700ms_gap() {
        let config = OverlayConfig::default();
        let now = Instant::now();
        let mut items = VecDeque::new();
        let mut next_id = 1;
        push_item(
            &mut items,
            event(
                OverlayInputKind::Shortcut,
                "Ctrl+C",
                OverlayCaptureSource::Local,
            ),
            now,
            &config,
            &mut next_id,
        );
        push_item(
            &mut items,
            event(
                OverlayInputKind::Shortcut,
                "Ctrl+C",
                OverlayCaptureSource::Local,
            ),
            now + Duration::from_millis(701),
            &config,
            &mut next_id,
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].repeat_count, 1);
        assert_eq!(items[1].repeat_count, 1);
    }

    #[test]
    fn max_rows_discards_oldest_logical_row_after_append() {
        let config = OverlayConfig {
            max_rows: 2,
            ..OverlayConfig::default()
        };
        let now = Instant::now();
        let mut items = VecDeque::new();
        let mut next_id = 1;
        for (key, offset) in [("A", 0), ("B", 1), ("C", 2)] {
            push_item(
                &mut items,
                event(
                    OverlayInputKind::Shortcut,
                    &format!("Ctrl+{key}"),
                    OverlayCaptureSource::Local,
                ),
                now + Duration::from_millis(offset),
                &config,
                &mut next_id,
            );
        }
        assert_eq!(items.len(), 2);
        assert_eq!(items.front().expect("oldest remaining").id, 2);
    }
}
