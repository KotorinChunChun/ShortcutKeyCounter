use std::sync::{mpsc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use crossbeam_channel::Sender;
use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PeekMessageW,
    PostThreadMessageW, RegisterClassW, HWND_MESSAGE, MSG, PBT_APMRESUMEAUTOMATIC,
    PBT_APMRESUMESUSPEND, PM_NOREMOVE, WM_POWERBROADCAST, WM_QUIT, WNDCLASSW,
};

const WINDOW_CLASS_NAME: &[u16] = &[
    b'S' as u16,
    b'h' as u16,
    b'o' as u16,
    b'r' as u16,
    b't' as u16,
    b'c' as u16,
    b'u' as u16,
    b't' as u16,
    b'P' as u16,
    b'o' as u16,
    b'w' as u16,
    b'e' as u16,
    b'r' as u16,
    b'M' as u16,
    b'o' as u16,
    b'n' as u16,
    b'i' as u16,
    b't' as u16,
    b'o' as u16,
    b'r' as u16,
    0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    Resume,
}

#[derive(Debug, Error)]
pub enum PowerMonitorError {
    #[error("power monitor threadを起動できません: {0}")]
    ThreadStart(String),
    #[error("power monitorを登録できません: {0}")]
    Registration(String),
    #[error("power monitor停止通知を送信できません: {0}")]
    StopNotification(String),
    #[error("power monitor threadが異常終了しました")]
    ThreadJoin,
}

/// 登録済みpower monitorを停止するためのhandle。
pub struct PowerMonitorHandle {
    thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

/// message-only windowで電源復帰通知の監視を開始する。
pub fn start_power_monitor(
    sender: Sender<PowerEvent>,
) -> Result<PowerMonitorHandle, PowerMonitorError> {
    let (ready_sender, ready_receiver) = mpsc::channel();
    let join_handle = thread::Builder::new()
        .name("shortcut-power-monitor".to_owned())
        .spawn(move || power_monitor_thread(sender, ready_sender))
        .map_err(|error| PowerMonitorError::ThreadStart(error.to_string()))?;

    match ready_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(PowerMonitorHandle {
            thread_id,
            join_handle: Some(join_handle),
        }),
        Ok(Err(error)) => {
            let _ = join_handle.join();
            Err(PowerMonitorError::Registration(error))
        }
        Err(error) => {
            let _ = join_handle.join();
            Err(PowerMonitorError::ThreadStart(error.to_string()))
        }
    }
}

/// power monitorを停止し、監視threadの終了を待つ。
pub fn stop_power_monitor(mut handle: PowerMonitorHandle) -> Result<(), PowerMonitorError> {
    unsafe { PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        .map_err(|error| PowerMonitorError::StopNotification(error.to_string()))?;

    if let Some(join_handle) = handle.join_handle.take() {
        join_handle
            .join()
            .map_err(|_| PowerMonitorError::ThreadJoin)?;
    }
    Ok(())
}

impl Drop for PowerMonitorHandle {
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

fn power_monitor_thread(
    sender: Sender<PowerEvent>,
    ready_sender: mpsc::Sender<Result<u32, String>>,
) {
    let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    let mut message = MSG::default();
    unsafe {
        // GetMessageWの前にmessage queueを作成し、停止通知を受けられるようにする。
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    }

    let hinstance = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => HINSTANCE(module.0),
        Err(error) => {
            let _ = ready_sender.send(Err(error.to_string()));
            return;
        }
    };

    if let Err(error) = register_window_class(hinstance) {
        let _ = ready_sender.send(Err(error));
        return;
    }

    let sender_context = power_monitor_sender();
    {
        let mut context = match sender_context.lock() {
            Ok(context) => context,
            Err(_) => {
                let _ = ready_sender.send(Err("power monitor contextがpoisonedです".to_owned()));
                return;
            }
        };
        if context.is_some() {
            let _ = ready_sender.send(Err("power monitorは既に起動しています".to_owned()));
            return;
        }
        *context = Some(sender);
    }

    let window = match unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(WINDOW_CLASS_NAME.as_ptr()),
            PCWSTR::null(),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        )
    } {
        Ok(window) => window,
        Err(error) => {
            clear_power_monitor_sender();
            let _ = ready_sender.send(Err(error.to_string()));
            return;
        }
    };

    if ready_sender.send(Ok(thread_id)).is_err() {
        let _ = unsafe { DestroyWindow(window) };
        clear_power_monitor_sender();
        return;
    }

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        unsafe { DispatchMessageW(&message) };
    }

    let _ = unsafe { DestroyWindow(window) };
    clear_power_monitor_sender();
}

fn register_window_class(hinstance: HINSTANCE) -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            let window_class = WNDCLASSW {
                hInstance: hinstance,
                lpfnWndProc: Some(power_window_proc),
                lpszClassName: PCWSTR(WINDOW_CLASS_NAME.as_ptr()),
                ..Default::default()
            };
            let atom = unsafe { RegisterClassW(&window_class) };
            if atom == 0 {
                Err("message-only window classの登録に失敗しました".to_owned())
            } else {
                Ok(())
            }
        })
        .clone()
}

unsafe extern "system" fn power_window_proc(
    window: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_POWERBROADCAST {
        if let Some(event) = power_event_from_message(message, wparam.0) {
            if let Ok(context) = power_monitor_sender().lock() {
                if let Some(sender) = context.as_ref() {
                    let _ = sender.send(event);
                }
            }
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn power_event_from_message(message: u32, wparam: usize) -> Option<PowerEvent> {
    if message == WM_POWERBROADCAST
        && matches!(wparam as u32, PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND)
    {
        Some(PowerEvent::Resume)
    } else {
        None
    }
}

fn power_monitor_sender() -> &'static Mutex<Option<Sender<PowerEvent>>> {
    static SENDER: OnceLock<Mutex<Option<Sender<PowerEvent>>>> = OnceLock::new();
    SENDER.get_or_init(|| Mutex::new(None))
}

fn clear_power_monitor_sender() {
    if let Ok(mut sender) = power_monitor_sender().lock() {
        *sender = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_resume_power_messages() {
        assert_eq!(
            power_event_from_message(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize),
            Some(PowerEvent::Resume)
        );
        assert_eq!(
            power_event_from_message(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND as usize),
            Some(PowerEvent::Resume)
        );
        assert_eq!(power_event_from_message(WM_POWERBROADCAST, 0), None);
        assert_eq!(
            power_event_from_message(WM_QUIT, PBT_APMRESUMEAUTOMATIC as usize),
            None
        );
    }

    #[test]
    #[ignore = "Windowsデスクトップ上で電源monitorの登録と停止を確認する手動スモークテスト"]
    fn power_monitor_can_start_and_stop_on_windows_desktop() {
        let (sender, _receiver) = crossbeam_channel::bounded(16);
        let handle = start_power_monitor(sender).expect("power monitor start");
        stop_power_monitor(handle).expect("power monitor stop");
    }
}
