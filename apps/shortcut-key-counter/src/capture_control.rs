use std::sync::atomic::{AtomicBool, Ordering};

use shortcut_windows::keyboard_hook::{
    start_keyboard_hook, stop_keyboard_hook, KeyboardHookError, KeyboardHookHandle,
};
use shortcut_windows::mouse_hook::{
    start_mouse_hook, stop_mouse_hook, MouseHookError, MouseHookHandle,
};
use thiserror::Error;

use crate::capture_worker::CaptureQueue;

/// 計測ON/OFFの制御エラー。
#[derive(Debug, Error)]
pub enum CaptureControlError {
    #[error("keyboard hookの開始に失敗しました: {0}")]
    KeyboardStart(#[source] KeyboardHookError),
    #[error("mouse hookの開始に失敗しました: {0}")]
    MouseStart(#[source] MouseHookError),
    #[error("keyboard hookの停止に失敗しました: {0}")]
    KeyboardStop(#[source] KeyboardHookError),
    #[error("mouse hookの停止に失敗しました: {0}")]
    MouseStop(#[source] MouseHookError),
}

/// 計測状態だけを保持するメモリ上のstate。
pub struct CaptureState {
    enabled: AtomicBool,
}

impl CaptureState {
    /// アプリ起動時の状態。計測状態は永続化しない。
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
        }
    }

    fn disabled() -> Self {
        Self {
            enabled: AtomicBool::new(false),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::new()
    }
}

/// keyboard／mouse hookと計測状態を一緒に管理する。
pub struct CaptureController {
    queue: CaptureQueue,
    state: CaptureState,
    keyboard_hook: Option<KeyboardHookHandle>,
    mouse_hook: Option<MouseHookHandle>,
}

impl CaptureController {
    /// 起動時に両hookを登録できた場合だけ、計測ONで返す。
    pub fn start(queue: CaptureQueue) -> Result<Self, CaptureControlError> {
        let mut controller = Self {
            queue,
            state: CaptureState::disabled(),
            keyboard_hook: None,
            mouse_hook: None,
        };
        controller.start_capture()?;
        Ok(controller)
    }

    pub fn is_enabled(&self) -> bool {
        self.state.is_enabled()
    }

    pub fn queue(&self) -> &CaptureQueue {
        &self.queue
    }

    /// OFF: hook解除→入力state clear→計測OFF。
    pub fn stop_capture(&mut self) -> Result<(), CaptureControlError> {
        let mut first_error = None;
        if let Some(handle) = self.keyboard_hook.take() {
            if let Err(error) = stop_keyboard_hook(handle) {
                first_error = Some(CaptureControlError::KeyboardStop(error));
            }
        }
        if let Some(handle) = self.mouse_hook.take() {
            if let Err(error) = stop_mouse_hook(handle) {
                if first_error.is_none() {
                    first_error = Some(CaptureControlError::MouseStop(error));
                }
            }
        }

        self.queue.clear_pending();
        self.state.disable();
        first_error.map_or(Ok(()), Err)
    }

    /// ON: state clear→両hook登録→両方成功後に計測ON。
    pub fn start_capture(&mut self) -> Result<(), CaptureControlError> {
        if self.state.is_enabled() {
            return Ok(());
        }

        self.queue.clear_pending();
        let keyboard_hook =
            start_keyboard_hook(self.queue.sender()).map_err(CaptureControlError::KeyboardStart)?;
        let mouse_hook = match start_mouse_hook(self.queue.sender()) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = stop_keyboard_hook(keyboard_hook);
                self.state.disable();
                return Err(CaptureControlError::MouseStart(error));
            }
        };

        self.keyboard_hook = Some(keyboard_hook);
        self.mouse_hook = Some(mouse_hook);
        self.state.enable();
        Ok(())
    }

    /// Resume後に、計測ONだった場合だけHookを再構築する。
    pub fn recover_capture_after_resume(&mut self) -> Result<(), CaptureControlError> {
        if !self.state.is_enabled() {
            return Ok(());
        }

        self.stop_capture()?;
        self.start_capture()
    }
}

impl Drop for CaptureController {
    fn drop(&mut self) {
        let _ = self.stop_capture();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_state_is_on_and_stop_start_state_is_memory_only() {
        let state = CaptureState::new();
        assert!(state.is_enabled());

        state.disable();
        assert!(!state.is_enabled());

        state.enable();
        assert!(state.is_enabled());
    }

    #[test]
    fn queue_pending_inputs_are_cleared_when_capture_stops() {
        let queue = CaptureQueue::with_capacity(1);
        let sender = queue.sender();
        sender
            .send(shortcut_core::model::CapturedInput {
                occurred_at: chrono::NaiveDate::from_ymd_opt(2026, 8, 15)
                    .expect("date")
                    .and_hms_milli_opt(9, 0, 0, 0)
                    .expect("time"),
                modifiers: shortcut_core::model::ModifierState {
                    ctrl: true,
                    alt: false,
                    win: false,
                    shift: false,
                },
                main_input: shortcut_core::model::MainInput::Key(0x43),
                foreground_executable: "Code.exe".to_owned(),
                pointer: None,
            })
            .expect("queue input");

        queue.clear_pending();
        assert!(queue.receiver().try_recv().is_err());
    }

    #[test]
    fn resume_recovery_does_not_turn_manually_disabled_capture_on() {
        let controller = CaptureController {
            queue: CaptureQueue::new(),
            state: CaptureState::disabled(),
            keyboard_hook: None,
            mouse_hook: None,
        };
        let mut controller = controller;

        controller
            .recover_capture_after_resume()
            .expect("disabled capture recovery");
        assert!(!controller.is_enabled());
    }

    #[test]
    #[ignore = "Windowsデスクトップ上でOFF→ONを含むHook再登録を確認する手動テスト"]
    fn controller_can_stop_and_restart_capture() {
        let queue = CaptureQueue::new();
        let mut controller = CaptureController::start(queue).expect("capture start");
        controller.stop_capture().expect("capture stop");
        assert!(!controller.is_enabled());
        controller.start_capture().expect("capture restart");
        assert!(controller.is_enabled());
    }
}
