use std::sync::{mpsc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use chrono::Local;
use crossbeam_channel::Sender;
use shortcut_core::model::{CapturedInput, MainInput, ModifierState, ScreenPoint};
use thiserror::Error;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::MSG;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, LLMHF_INJECTED, MSLLHOOKSTRUCT, PM_NOREMOVE, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

use crate::foreground::foreground_executable_name;

static MOUSE_HOOK_CONTEXT: OnceLock<Mutex<Option<MouseHookContext>>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum MouseHookError {
    #[error("mouse hookは既に起動しています")]
    AlreadyRunning,
    #[error("mouse hook threadを起動できません: {0}")]
    ThreadStart(String),
    #[error("mouse hookを登録できません: {0}")]
    Registration(String),
    #[error("mouse hook停止通知を送信できません: {0}")]
    StopNotification(String),
    #[error("mouse hook threadが異常終了しました")]
    ThreadJoin,
}

struct MouseHookContext {
    sender: Sender<CapturedInput>,
}

pub struct MouseHookHandle {
    thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

/// low-level mouse hookを登録する。
pub fn start_mouse_hook(sender: Sender<CapturedInput>) -> Result<MouseHookHandle, MouseHookError> {
    let context_lock = mouse_hook_context();
    {
        let mut context = context_lock
            .lock()
            .map_err(|_| MouseHookError::ThreadStart("hook contextがpoisonedです".to_owned()))?;
        if context.is_some() {
            return Err(MouseHookError::AlreadyRunning);
        }
        *context = Some(MouseHookContext { sender });
    }

    let (ready_sender, ready_receiver) = mpsc::channel();
    let join_handle = match thread::Builder::new()
        .name("shortcut-mouse-hook".to_owned())
        .spawn(move || mouse_hook_thread(ready_sender))
    {
        Ok(handle) => handle,
        Err(error) => {
            clear_mouse_hook_context();
            return Err(MouseHookError::ThreadStart(error.to_string()));
        }
    };

    match ready_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(MouseHookHandle {
            thread_id,
            join_handle: Some(join_handle),
        }),
        Ok(Err(error)) => {
            let _ = join_handle.join();
            Err(MouseHookError::Registration(error))
        }
        Err(error) => {
            let _ = join_handle.join();
            Err(MouseHookError::ThreadStart(error.to_string()))
        }
    }
}

/// mouse hookを停止し、登録threadの終了を待つ。
pub fn stop_mouse_hook(mut handle: MouseHookHandle) -> Result<(), MouseHookError> {
    unsafe { PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        .map_err(|error| MouseHookError::StopNotification(error.to_string()))?;

    if let Some(join_handle) = handle.join_handle.take() {
        join_handle.join().map_err(|_| MouseHookError::ThreadJoin)?;
    }
    Ok(())
}

impl Drop for MouseHookHandle {
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

fn mouse_hook_thread(ready_sender: mpsc::Sender<Result<u32, String>>) {
    let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let mut message = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    }

    let hook = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) } {
        Ok(hook) => hook,
        Err(error) => {
            clear_mouse_hook_context();
            let _ = ready_sender.send(Err(error.to_string()));
            return;
        }
    };

    if ready_sender.send(Ok(thread_id)).is_err() {
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        clear_mouse_hook_context();
        return;
    }

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
    }

    let _ = unsafe { UnhookWindowsHookEx(hook) };
    clear_mouse_hook_context();
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let data = lparam.0 as *const MSLLHOOKSTRUCT;
        if !data.is_null() {
            let hook_data = unsafe { *data };
            if hook_data.flags & LLMHF_INJECTED == 0 {
                if let Some((sender, input)) = process_mouse_message(wparam.0 as u32, &hook_data) {
                    let _ = sender.try_send(input);
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn process_mouse_message(
    message: u32,
    hook_data: &MSLLHOOKSTRUCT,
) -> Option<(Sender<CapturedInput>, CapturedInput)> {
    let main_input = map_mouse_message(message, hook_data.mouseData)?;
    let modifiers = current_modifier_state();

    let sender = mouse_hook_context().lock().ok()?.as_ref()?.sender.clone();
    let application = foreground_executable_name().ok()?;
    let candidate = build_candidate(
        modifiers,
        main_input,
        application,
        Local::now().naive_local(),
        ScreenPoint {
            x: hook_data.pt.x,
            y: hook_data.pt.y,
        },
    );
    Some((sender, candidate))
}

fn map_mouse_message(message: u32, mouse_data: u32) -> Option<MainInput> {
    match message {
        WM_LBUTTONDOWN => Some(MainInput::MouseLeft),
        WM_RBUTTONDOWN => Some(MainInput::MouseRight),
        WM_MBUTTONDOWN => Some(MainInput::MouseMiddle),
        WM_XBUTTONDOWN => match (mouse_data >> 16) as u16 {
            1 => Some(MainInput::MouseX1),
            2 => Some(MainInput::MouseX2),
            _ => None,
        },
        WM_MOUSEWHEEL => {
            let delta = (mouse_data >> 16) as i16;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => Some(MainInput::WheelUp),
                std::cmp::Ordering::Less => Some(MainInput::WheelDown),
                std::cmp::Ordering::Equal => None,
            }
        }
        WM_MOUSEHWHEEL => {
            let delta = (mouse_data >> 16) as i16;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => Some(MainInput::WheelRight),
                std::cmp::Ordering::Less => Some(MainInput::WheelLeft),
                std::cmp::Ordering::Equal => None,
            }
        }
        _ => None,
    }
}

fn current_modifier_state() -> ModifierState {
    ModifierState {
        ctrl: is_key_pressed(0x11, 0xA2, 0xA3),
        alt: is_key_pressed(0x12, 0xA4, 0xA5),
        win: is_key_pressed(0x5B, 0x5B, 0x5C),
        shift: is_key_pressed(0x10, 0xA0, 0xA1),
    }
}

fn is_key_pressed(generic: i32, left: i32, right: i32) -> bool {
    (unsafe { GetAsyncKeyState(generic) < 0 })
        || (unsafe { GetAsyncKeyState(left) < 0 })
        || (unsafe { GetAsyncKeyState(right) < 0 })
}

fn build_candidate(
    modifiers: ModifierState,
    main_input: MainInput,
    application: String,
    occurred_at: chrono::NaiveDateTime,
    pointer: ScreenPoint,
) -> CapturedInput {
    CapturedInput {
        occurred_at,
        modifiers,
        main_input,
        foreground_executable: application,
        pointer: Some(pointer),
    }
}

fn mouse_hook_context() -> &'static Mutex<Option<MouseHookContext>> {
    MOUSE_HOOK_CONTEXT.get_or_init(|| Mutex::new(None))
}

fn clear_mouse_hook_context() {
    if let Ok(mut context) = mouse_hook_context().lock() {
        *context = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::POINT;

    fn hook_data(mouse_data: u32) -> MSLLHOOKSTRUCT {
        MSLLHOOKSTRUCT {
            pt: POINT { x: 0, y: 0 },
            mouseData: mouse_data,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        }
    }

    #[test]
    fn maps_left_right_and_wheel_directions() {
        assert_eq!(
            map_mouse_message(WM_LBUTTONDOWN, 0),
            Some(MainInput::MouseLeft)
        );
        assert_eq!(
            map_mouse_message(WM_RBUTTONDOWN, 0),
            Some(MainInput::MouseRight)
        );
        assert_eq!(
            map_mouse_message(WM_MOUSEWHEEL, 120_u32 << 16),
            Some(MainInput::WheelUp)
        );
        assert_eq!(
            map_mouse_message(WM_MOUSEWHEEL, ((-120_i16 as u16) as u32) << 16),
            Some(MainInput::WheelDown)
        );
        assert_eq!(
            map_mouse_message(WM_XBUTTONDOWN, 1_u32 << 16),
            Some(MainInput::MouseX1)
        );
        assert_eq!(
            map_mouse_message(WM_XBUTTONDOWN, 2_u32 << 16),
            Some(MainInput::MouseX2)
        );
        assert_eq!(
            map_mouse_message(WM_MOUSEHWHEEL, 120_u32 << 16),
            Some(MainInput::WheelRight)
        );
        assert_eq!(
            map_mouse_message(WM_MOUSEHWHEEL, ((-120_i16 as u16) as u32) << 16),
            Some(MainInput::WheelLeft)
        );
    }

    #[test]
    fn drops_non_target_mouse_messages_and_zero_delta() {
        assert_eq!(
            map_mouse_message(WM_MBUTTONDOWN, 0),
            Some(MainInput::MouseMiddle)
        );
        assert_eq!(map_mouse_message(WM_XBUTTONDOWN, 0), None);
        assert_eq!(map_mouse_message(WM_MOUSEWHEEL, 0), None);
        assert_eq!(map_mouse_message(WM_LBUTTONDOWN + 1, 0), None);
    }

    #[test]
    fn candidate_keeps_modifier_snapshot_and_mouse_kind() {
        let occurred_at = chrono::NaiveDate::from_ymd_opt(2026, 8, 15)
            .expect("date")
            .and_hms_milli_opt(9, 20, 44, 211)
            .expect("time");
        let candidate = build_candidate(
            ModifierState {
                ctrl: true,
                alt: false,
                win: false,
                shift: false,
            },
            MainInput::MouseLeft,
            "EXCEL.EXE".to_owned(),
            occurred_at,
            ScreenPoint { x: 10, y: 20 },
        );

        assert_eq!(candidate.main_input, MainInput::MouseLeft);
        assert!(candidate.modifiers.ctrl);
        assert_eq!(candidate.foreground_executable, "EXCEL.EXE");
    }

    #[test]
    fn injected_flag_is_available_for_callback_filtering() {
        let data = hook_data(0);
        assert_eq!(data.flags & LLMHF_INJECTED, 0);
    }

    #[test]
    #[ignore = "Windowsデスクトップ上で100回の登録／解除を確認する手動スモークテスト"]
    fn hook_can_start_and_stop_on_windows_desktop() {
        let (sender, _receiver) = crossbeam_channel::bounded(1024);
        let handle = start_mouse_hook(sender).expect("mouse hook start");
        stop_mouse_hook(handle).expect("mouse hook stop");
    }
}
