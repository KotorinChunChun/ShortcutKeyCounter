use std::sync::{mpsc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use chrono::Local;
use crossbeam_channel::Sender;
use shortcut_core::model::{CapturedInput, MainInput, ModifierState as CoreModifierState};
use thiserror::Error;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::MSG;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::foreground::foreground_executable_name;
use crate::modifier_state::{
    ModifierStateTracker, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_RWIN,
};
use crate::pressed_keys::PressedKeys;

static HOOK_CONTEXT: OnceLock<Mutex<Option<HookContext>>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum KeyboardHookError {
    #[error("keyboard hookは既に起動しています")]
    AlreadyRunning,
    #[error("keyboard hook threadを起動できません: {0}")]
    ThreadStart(String),
    #[error("keyboard hookを登録できません: {0}")]
    Registration(String),
    #[error("keyboard hook停止通知を送信できません: {0}")]
    StopNotification(String),
    #[error("keyboard hook threadが異常終了しました")]
    ThreadJoin,
}

struct HookContext {
    sender: Sender<CapturedInput>,
    modifiers: ModifierStateTracker,
    pressed_keys: PressedKeys,
}

impl HookContext {
    fn new(sender: Sender<CapturedInput>) -> Self {
        Self {
            sender,
            modifiers: ModifierStateTracker::new(),
            pressed_keys: PressedKeys::new(),
        }
    }
}

/// 登録済みkeyboard hookを停止するためのhandle。
pub struct KeyboardHookHandle {
    thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

/// low-level keyboard hookを登録する。
pub fn start_keyboard_hook(
    sender: Sender<CapturedInput>,
) -> Result<KeyboardHookHandle, KeyboardHookError> {
    let context_lock = hook_context();
    {
        let mut context = context_lock
            .lock()
            .map_err(|_| KeyboardHookError::ThreadStart("hook contextがpoisonedです".to_owned()))?;
        if context.is_some() {
            return Err(KeyboardHookError::AlreadyRunning);
        }
        *context = Some(HookContext::new(sender));
    }

    let (ready_sender, ready_receiver) = mpsc::channel();
    let join_handle = match thread::Builder::new()
        .name("shortcut-keyboard-hook".to_owned())
        .spawn(move || keyboard_hook_thread(ready_sender))
    {
        Ok(handle) => handle,
        Err(error) => {
            clear_hook_context();
            return Err(KeyboardHookError::ThreadStart(error.to_string()));
        }
    };

    match ready_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(KeyboardHookHandle {
            thread_id,
            join_handle: Some(join_handle),
        }),
        Ok(Err(error)) => {
            let _ = join_handle.join();
            Err(KeyboardHookError::Registration(error))
        }
        Err(error) => {
            let _ = join_handle.join();
            Err(KeyboardHookError::ThreadStart(error.to_string()))
        }
    }
}

/// keyboard hookを停止し、登録threadの終了を待つ。
pub fn stop_keyboard_hook(mut handle: KeyboardHookHandle) -> Result<(), KeyboardHookError> {
    let notification =
        unsafe { PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
            .map_err(|error| KeyboardHookError::StopNotification(error.to_string()));

    notification?;
    if let Some(join_handle) = handle.join_handle.take() {
        join_handle
            .join()
            .map_err(|_| KeyboardHookError::ThreadJoin)?;
    }
    Ok(())
}

impl Drop for KeyboardHookHandle {
    fn drop(&mut self) {
        if self.join_handle.is_none() {
            return;
        }

        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

fn keyboard_hook_thread(ready_sender: mpsc::Sender<Result<u32, String>>) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    unsafe {
        // GetMessageWの前にmessage queueを作成し、callerからの停止通知を受けられるようにする。
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    }

    let hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) }
    {
        Ok(hook) => hook,
        Err(error) => {
            clear_hook_context();
            let _ = ready_sender.send(Err(error.to_string()));
            return;
        }
    };

    if ready_sender.send(Ok(thread_id)).is_err() {
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        clear_hook_context();
        return;
    }

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
    }

    let _ = unsafe { UnhookWindowsHookEx(hook) };
    clear_hook_context();
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        let data = lparam.0 as *const KBDLLHOOKSTRUCT;
        if !data.is_null() {
            let hook_data = unsafe { *data };
            if !hook_data.flags.contains(LLKHF_INJECTED) {
                if let Some((sender, input)) = process_keyboard_message(message, hook_data.vkCode) {
                    let _ = sender.try_send(input);
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn process_keyboard_message(
    message: u32,
    virtual_key: u32,
) -> Option<(Sender<CapturedInput>, CapturedInput)> {
    let virtual_key = u16::try_from(virtual_key).ok()?;
    let context_lock = hook_context();
    let (sender, modifiers) = {
        let mut context = context_lock.lock().ok()?;
        let context = context.as_mut()?;

        if is_key_down_message(message) {
            if is_modifier_key(virtual_key) {
                context.modifiers.key_down(virtual_key);
                return None;
            }
            if !context.pressed_keys.key_down(virtual_key) {
                return None;
            }
            let modifiers = context
                .modifiers
                .fold(|ctrl, alt, win, shift| CoreModifierState {
                    ctrl,
                    alt,
                    win,
                    shift,
                });
            (context.sender.clone(), modifiers)
        } else if is_key_up_message(message) {
            if is_modifier_key(virtual_key) {
                context.modifiers.key_up(virtual_key);
            } else {
                context.pressed_keys.key_up(virtual_key);
            }
            return None;
        } else {
            return None;
        }
    };

    let application = foreground_executable_name().ok()?;
    let input = build_candidate(
        modifiers,
        virtual_key,
        application,
        Local::now().naive_local(),
    );
    Some((sender, input))
}

fn build_candidate(
    modifiers: CoreModifierState,
    virtual_key: u16,
    application: String,
    occurred_at: chrono::NaiveDateTime,
) -> CapturedInput {
    CapturedInput {
        occurred_at,
        modifiers,
        main_input: MainInput::Key(virtual_key),
        foreground_executable: application,
        pointer: None,
    }
}

fn is_key_down_message(message: u32) -> bool {
    matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
}

fn is_key_up_message(message: u32) -> bool {
    matches!(message, WM_KEYUP | WM_SYSKEYUP)
}

fn is_modifier_key(virtual_key: u16) -> bool {
    matches!(
        virtual_key,
        VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
            | VK_LSHIFT
            | VK_RSHIFT
            | 0x10
            | 0x11
            | 0x12
    )
}

fn hook_context() -> &'static Mutex<Option<HookContext>> {
    HOOK_CONTEXT.get_or_init(|| Mutex::new(None))
}

fn clear_hook_context() {
    if let Ok(mut context) = hook_context().lock() {
        *context = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_keyboard_message_categories() {
        assert!(is_key_down_message(WM_KEYDOWN));
        assert!(is_key_down_message(WM_SYSKEYDOWN));
        assert!(is_key_up_message(WM_KEYUP));
        assert!(is_key_up_message(WM_SYSKEYUP));
        assert!(!is_key_down_message(WM_KEYUP));
        assert!(!is_key_up_message(WM_KEYDOWN));
    }

    #[test]
    fn recognizes_generic_and_left_right_modifier_vk_codes() {
        assert!(is_modifier_key(0x11));
        assert!(is_modifier_key(VK_LCONTROL));
        assert!(is_modifier_key(VK_RSHIFT));
        assert!(!is_modifier_key(0x43));
    }

    #[test]
    fn candidate_contains_only_allowed_capture_fields() {
        let occurred_at = chrono::NaiveDate::from_ymd_opt(2026, 8, 15)
            .expect("date")
            .and_hms_milli_opt(9, 14, 21, 152)
            .expect("time");
        let candidate = build_candidate(
            CoreModifierState {
                ctrl: true,
                alt: false,
                win: false,
                shift: false,
            },
            0x43,
            "Code.exe".to_owned(),
            occurred_at,
        );

        assert_eq!(candidate.main_input, MainInput::Key(0x43));
        assert_eq!(candidate.foreground_executable, "Code.exe");
        assert_eq!(candidate.occurred_at, occurred_at);
    }

    #[test]
    #[ignore = "Windowsデスクトップ上で100回の登録／解除を確認する手動スモークテスト"]
    fn hook_can_start_and_stop_on_windows_desktop() {
        let (sender, _receiver) = crossbeam_channel::bounded(1024);
        let handle = start_keyboard_hook(sender).expect("keyboard hook start");
        stop_keyboard_hook(handle).expect("keyboard hook stop");
    }
}
