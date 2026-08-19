use std::path::Path;

use thiserror::Error;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::OpenProcessToken;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Foreground executableを解決できなかった理由。
///
/// この型はshortcut文字列やwindow titleなどの入力情報を保持しない。
#[derive(Debug, Error)]
pub enum ForegroundError {
    #[error("foreground windowを取得できません")]
    NoForegroundWindow,
    #[error("foreground windowのprocess idを取得できません")]
    ProcessIdUnavailable,
    #[error("processを開けません: {0}")]
    OpenProcess(#[source] windows::core::Error),
    #[error("process image pathを取得できません: {0}")]
    QueryImagePath(#[source] windows::core::Error),
    #[error("process image pathが空です")]
    EmptyImagePath,
    #[error("process image pathから実行ファイル名を取得できません")]
    ExecutableNameUnavailable,
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // CloseHandleの失敗は、元のprocess情報を返す処理を覆すものではない。
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// 現在foregroundにあるウィンドウのprocess idを取得する。
pub fn foreground_process_id() -> Result<u32, ForegroundError> {
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return Err(ForegroundError::NoForegroundWindow);
    }

    let mut process_id = 0_u32;
    let thread_id = unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    if thread_id == 0 || process_id == 0 {
        return Err(ForegroundError::ProcessIdUnavailable);
    }

    Ok(process_id)
}

/// 指定processがUAC昇格済みかを返す。取得不能時はエラーとして呼び出し側でFail Closedにできる。
pub fn process_is_elevated(process_id: u32) -> Result<bool, ForegroundError> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(ForegroundError::OpenProcess)?;
    let _process_guard = ProcessHandle(process);
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }
        .map_err(ForegroundError::OpenProcess)?;
    let _token_guard = ProcessHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0_u32;
    unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .map_err(ForegroundError::OpenProcess)?;
    Ok(elevation.TokenIsElevated != 0)
}

pub fn foreground_process_is_elevated() -> Result<bool, ForegroundError> {
    process_is_elevated(foreground_process_id()?)
}

/// process idから実行ファイル名だけを解決する。
pub fn executable_name_for_process(process_id: u32) -> Result<String, ForegroundError> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(ForegroundError::OpenProcess)?;
    let _handle_guard = ProcessHandle(handle);

    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(ForegroundError::QueryImagePath)?;

    buffer.truncate(length as usize);
    let image_path = String::from_utf16_lossy(&buffer);
    executable_name_from_path(&image_path)
}

/// foreground processの実行ファイル名を取得する。
pub fn foreground_executable_name() -> Result<String, ForegroundError> {
    let process_id = foreground_process_id()?;
    executable_name_for_process(process_id)
}

fn executable_name_from_path(image_path: &str) -> Result<String, ForegroundError> {
    if image_path.is_empty() {
        return Err(ForegroundError::EmptyImagePath);
    }

    Path::new(image_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or(ForegroundError::ExecutableNameUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_name_excludes_directory_path() {
        assert_eq!(
            executable_name_from_path(r"C:\Windows\explorer.exe").unwrap(),
            "explorer.exe"
        );
        assert_eq!(
            executable_name_from_path(r"C:\Program Files\Microsoft\msedge.exe").unwrap(),
            "msedge.exe"
        );
    }

    #[test]
    fn empty_image_path_returns_error_without_sensitive_data() {
        let error = executable_name_from_path("").expect_err("empty path must fail");
        assert!(matches!(error, ForegroundError::EmptyImagePath));
        assert!(!error.to_string().contains("Ctrl"));
    }

    #[test]
    fn invalid_process_id_returns_error() {
        let result = executable_name_for_process(0);
        assert!(result.is_err());
    }
}
