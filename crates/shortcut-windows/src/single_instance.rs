//! 名前付きMutexとEventによるShortcutKeyCounterの単一起動制御。
//!
//! 2個目のプロセスはbackendやHookを初期化せず、既存プロセスへ
//! 「設定画面を表示」のactivation eventだけ送って終了する。

use std::ffi::c_void;

use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject,
};

const MUTEX_NAME: &str = r"Local\ShortcutKeyCounter.Singleton.v1";
const ACTIVATE_EVENT_NAME: &str = r"Local\ShortcutKeyCounter.ActivateSettings.v1";

#[derive(Debug, Error)]
pub enum SingleInstanceError {
    #[error("単一起動Mutex/EventのWindows APIに失敗しました: {0}")]
    Windows(#[from] windows::core::Error),
}

pub enum SingleInstanceStatus {
    Primary(SingleInstanceGuard),
    ExistingNotified,
}

pub struct SingleInstanceGuard {
    mutex_value: isize,
    event_value: isize,
}

impl SingleInstanceGuard {
    fn mutex_handle(&self) -> HANDLE {
        HANDLE(self.mutex_value as *mut c_void)
    }

    fn event_handle(&self) -> HANDLE {
        HANDLE(self.event_value as *mut c_void)
    }

    /// activation listenerへ渡すprocess-wide event handle値。
    pub fn activation_event_value(&self) -> isize {
        self.event_value
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.event_handle()) };
        let _ = unsafe { CloseHandle(self.mutex_handle()) };
    }
}

/// 最初のプロセスならGuardを返す。既存プロセスがあればactivation eventを通知して返す。
pub fn acquire_or_notify_existing() -> Result<SingleInstanceStatus, SingleInstanceError> {
    // Eventを先に作ることで、ほぼ同時起動でも2個目がactivationを通知できる。
    let event_name = wide_null(ACTIVATE_EVENT_NAME);
    let event = unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }?;

    let mutex_name = wide_null(MUTEX_NAME);
    let mutex = match unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) } {
        Ok(mutex) => mutex,
        Err(error) => {
            let _ = unsafe { CloseHandle(event) };
            return Err(error.into());
        }
    };
    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

    if already_exists {
        let _ = unsafe { SetEvent(event) };
        let _ = unsafe { CloseHandle(event) };
        let _ = unsafe { CloseHandle(mutex) };
        Ok(SingleInstanceStatus::ExistingNotified)
    } else {
        Ok(SingleInstanceStatus::Primary(SingleInstanceGuard {
            mutex_value: mutex.0 as isize,
            event_value: event.0 as isize,
        }))
    }
}

/// Primary processのlistener threadから呼ぶ。timeout内にactivationがあればtrue。
pub fn wait_for_activation(event_value: isize, timeout_ms: u32) -> bool {
    let event = HANDLE(event_value as *mut c_void);
    let wait_result = unsafe { WaitForSingleObject(event, timeout_ms) };
    wait_result == WAIT_OBJECT_0
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_names_are_per_user_session_and_versioned() {
        assert!(MUTEX_NAME.starts_with("Local\\"));
        assert!(ACTIVATE_EVENT_NAME.starts_with("Local\\"));
        assert!(MUTEX_NAME.ends_with("v1"));
    }
}
