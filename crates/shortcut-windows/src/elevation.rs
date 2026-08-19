//! UACのrunas動詞によるcapture helper起動。

use std::path::Path;

use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const SHELL_EXECUTE_MIN_SUCCESS: isize = 32;

#[derive(Debug, Error)]
pub enum ElevationError {
    #[error("helper実行ファイルのパスが空です")]
    EmptyExecutablePath,
    #[error("UACによるhelper起動がキャンセルされました")]
    Cancelled,
    #[error("UAC helperの起動に失敗しました: ShellExecuteエラー {0}")]
    ShellExecute(u32),
}

/// helperへ渡すコマンドライン引数を生成する。
pub fn helper_arguments(pipe_name: &str, token: &str) -> Result<String, ElevationError> {
    if pipe_name.is_empty() || token.is_empty() {
        return Err(ElevationError::EmptyExecutablePath);
    }
    Ok(format!("--pipe \"{}\" --token \"{}\"", pipe_name, token))
}

/// ShellExecuteのrunas動詞でhelperを管理者権限起動する。
pub fn launch_helper_elevated(
    executable_path: impl AsRef<Path>,
    pipe_name: &str,
    token: &str,
) -> Result<(), ElevationError> {
    let executable_path = executable_path.as_ref();
    if executable_path.as_os_str().is_empty() {
        return Err(ElevationError::EmptyExecutablePath);
    }
    let arguments = helper_arguments(pipe_name, token)?;
    let operation = wide_null("runas");
    let executable = wide_null(&executable_path.to_string_lossy());
    let arguments = wide_null(&arguments);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(executable.as_ptr()),
            PCWSTR(arguments.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };
    let status = result.0 as isize;
    if status <= SHELL_EXECUTE_MIN_SUCCESS {
        if status == ERROR_ACCESS_DENIED.0 as isize {
            return Err(ElevationError::Cancelled);
        }
        return Err(ElevationError::ShellExecute(status.max(0) as u32));
    }
    Ok(())
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_arguments_quote_pipe_and_token() {
        assert_eq!(
            helper_arguments(r"\\.\pipe\ShortcutKeyCounter-capture-test", "token")
                .expect("arguments"),
            r#"--pipe "\\.\pipe\ShortcutKeyCounter-capture-test" --token "token""#
        );
    }

    #[test]
    fn empty_pipe_or_token_is_rejected() {
        assert!(matches!(
            helper_arguments("", "token"),
            Err(ElevationError::EmptyExecutablePath)
        ));
        assert!(matches!(
            helper_arguments("pipe", ""),
            Err(ElevationError::EmptyExecutablePath)
        ));
    }

    #[test]
    #[ignore = "UACダイアログを表示するWindows手動受入テスト"]
    fn launch_helper_uses_runas() {
        launch_helper_elevated(
            r"C:\ShortcutKeyCounter\ShortcutKeyCapture.exe",
            r"\\.\pipe\ShortcutKeyCounter-capture-test",
            "token",
        )
        .expect("runas launch");
    }
}
