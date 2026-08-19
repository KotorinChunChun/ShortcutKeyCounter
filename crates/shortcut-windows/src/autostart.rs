//! HKCU Runキーによる自動起動の読み書き。

use std::path::Path;

use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

pub const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE_NAME: &str = "ShortcutKeyCounter";
pub const BACKGROUND_ARGUMENT: &str = "-hide";

#[derive(Debug, Error)]
pub enum AutostartError {
    #[error("レジストリ操作に失敗しました: Win32エラー {0}")]
    Registry(u32),
    #[error("自動起動値の型がREG_SZではありません")]
    UnexpectedValueType,
    #[error("自動起動値をUTF-16から復元できません")]
    InvalidUtf16,
    #[error("実行ファイルパスが空です")]
    EmptyExecutablePath,
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }
}

/// 現在のexeパスをRunキーへ登録する値へ変換する。
pub fn autostart_command(executable_path: impl AsRef<Path>) -> Result<String, AutostartError> {
    let executable_path = executable_path.as_ref();
    if executable_path.as_os_str().is_empty() {
        return Err(AutostartError::EmptyExecutablePath);
    }

    Ok(format!(
        "\"{}\" {BACKGROUND_ARGUMENT}",
        executable_path.to_string_lossy()
    ))
}

/// Runキーの現在値を取得する。値が存在しない場合はNoneを返す。
pub fn read_autostart_command() -> Result<Option<String>, AutostartError> {
    let key = match open_run_key(KEY_QUERY_VALUE.0) {
        Ok(key) => key,
        Err(AutostartError::Registry(code)) if code == ERROR_FILE_NOT_FOUND.0 => return Ok(None),
        Err(error) => return Err(error),
    };

    let value_name = wide_null(RUN_VALUE_NAME);
    let mut value_type = REG_SZ;
    let mut byte_length = 0_u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut byte_length),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    ensure_success(status)?;
    if value_type != REG_SZ {
        return Err(AutostartError::UnexpectedValueType);
    }

    let mut bytes = vec![0_u8; byte_length as usize];
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            Some(bytes.as_mut_ptr()),
            Some(&mut byte_length),
        )
    };
    ensure_success(status)?;
    bytes.truncate(byte_length as usize);
    if !bytes.len().is_multiple_of(std::mem::size_of::<u16>()) {
        return Err(AutostartError::InvalidUtf16);
    }

    let utf16 = bytes
        .chunks_exact(std::mem::size_of::<u16>())
        .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
        .take_while(|value| *value != 0)
        .collect::<Vec<_>>();
    String::from_utf16(&utf16)
        .map(Some)
        .map_err(|_| AutostartError::InvalidUtf16)
}

/// 指定したexeパスを現在ユーザーのRunキーへ登録する。
pub fn enable_autostart(executable_path: impl AsRef<Path>) -> Result<(), AutostartError> {
    let command = autostart_command(executable_path)?;
    let key = create_run_key()?;
    let value_name = wide_null(RUN_VALUE_NAME);
    let value = wide_null(&command);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            value.as_ptr().cast::<u8>(),
            value.len() * std::mem::size_of::<u16>(),
        )
    };
    let status = unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(value_name.as_ptr()),
            None,
            REG_SZ,
            Some(bytes),
        )
    };
    ensure_success(status)
}

/// 現在ユーザーのRunキーからアプリの値を削除する。
pub fn disable_autostart() -> Result<(), AutostartError> {
    let key = match open_run_key(KEY_SET_VALUE.0) {
        Ok(key) => key,
        Err(AutostartError::Registry(code)) if code == ERROR_FILE_NOT_FOUND.0 => return Ok(()),
        Err(error) => return Err(error),
    };
    let value_name = wide_null(RUN_VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key.0, PCWSTR(value_name.as_ptr())) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    ensure_success(status)
}

fn open_run_key(access: u32) -> Result<RegistryKey, AutostartError> {
    let path = wide_null(RUN_KEY_PATH);
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            if access == KEY_QUERY_VALUE.0 {
                KEY_QUERY_VALUE
            } else {
                KEY_SET_VALUE
            },
            &mut key,
        )
    };
    ensure_success(status)?;
    Ok(RegistryKey(key))
}

fn create_run_key() -> Result<RegistryKey, AutostartError> {
    let path = wide_null(RUN_KEY_PATH);
    let mut key = HKEY::default();
    let status = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), &mut key) };
    ensure_success(status)?;
    Ok(RegistryKey(key))
}

fn ensure_success(status: windows::Win32::Foundation::WIN32_ERROR) -> Result<(), AutostartError> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(AutostartError::Registry(status.0))
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn command_quotes_executable_and_adds_background_argument() {
        assert_eq!(
            autostart_command(r"C:\Program Files\ShortcutKeyCounter\ShortcutKeyCounter.exe")
                .expect("command"),
            r#""C:\Program Files\ShortcutKeyCounter\ShortcutKeyCounter.exe" -hide"#
        );
    }

    #[test]
    fn empty_executable_path_is_rejected() {
        assert!(matches!(
            autostart_command(PathBuf::new()),
            Err(AutostartError::EmptyExecutablePath)
        ));
    }

    #[test]
    #[ignore = "WindowsユーザーのRunキーを書き換える手動受入テスト"]
    fn registry_autostart_can_be_enabled_read_and_disabled() {
        let executable = PathBuf::from(r"C:\ShortcutKeyCounter\ShortcutKeyCounter.exe");
        enable_autostart(&executable).expect("enable");
        assert_eq!(
            read_autostart_command().expect("read"),
            Some(autostart_command(&executable).expect("command"))
        );
        disable_autostart().expect("disable");
        assert!(read_autostart_command()
            .expect("read after disable")
            .is_none());
    }
}
