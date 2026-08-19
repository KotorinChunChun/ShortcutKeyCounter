use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Debug, Error)]
pub enum ShellOpenError {
    #[error("Windows Shellで対象を開けませんでした (ShellExecuteW={0})")]
    Failed(isize),
}

pub fn open_path(path: &Path) -> Result<(), ShellOpenError> {
    open_target(path.as_os_str())
}

pub fn open_url(url: &str) -> Result<(), ShellOpenError> {
    open_target(OsStr::new(url))
}

fn open_target(target: &OsStr) -> Result<(), ShellOpenError> {
    let operation = wide_null(OsStr::new("open"));
    let target = wide_null(target);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        Err(ShellOpenError::Failed(code))
    } else {
        Ok(())
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
