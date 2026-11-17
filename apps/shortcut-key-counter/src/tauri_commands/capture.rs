//! 計測ON/OFF、Capture経路の健全性、管理者権限Captureの切替。

use shortcut_windows::elevation::ElevationError;
use tauri::{AppHandle, Manager, State};

use super::settings_snapshot;
use crate::app_state::{AppState, AppStateError};
use crate::capture_health::CaptureHealthSnapshot;
use crate::command_api::{CaptureStatus, SettingsSnapshot};
use crate::command_error::CommandError;
use crate::elevated_capture::ElevatedCaptureError;
use crate::tray::sync_capture_ui;

fn capture_status(state: &AppState) -> Result<CaptureStatus, CommandError> {
    Ok(CaptureStatus {
        enabled: state.capture_enabled().map_err(|_| CommandError::state())?,
        dropped_queue_items: state
            .dropped_queue_items()
            .map_err(|_| CommandError::state())?,
    })
}

#[tauri::command]
pub fn get_capture_status(state: State<'_, AppState>) -> Result<CaptureStatus, CommandError> {
    capture_status(&state)
}

#[tauri::command]
pub fn set_capture_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<CaptureStatus, CommandError> {
    state.set_capture_enabled(enabled).map_err(|_| {
        CommandError::new(
            if enabled {
                "CAPTURE_START_FAILED"
            } else {
                "CAPTURE_STOP_FAILED"
            },
            if enabled {
                "計測を開始できませんでした。"
            } else {
                "計測を停止できませんでした。"
            },
        )
    })?;
    sync_capture_ui(&app, enabled);
    capture_status(&state)
}

#[tauri::command]
pub fn get_capture_health(state: State<'_, AppState>) -> CaptureHealthSnapshot {
    state.capture_health()
}

#[tauri::command]
pub async fn set_capture_elevated(
    app: AppHandle,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    // UAC / Named Pipe handshake / cleanupはblocking Windows APIを含むため、
    // Tauriのblocking poolへ退避してWebView event loopを止めない。
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state
            .set_elevated_enabled(enabled)
            .map_err(|error| elevated_switch_error(&error))?;
        settings_snapshot(&state)
    })
    .await
    .map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "管理者権限モードの処理スレッドが異常終了しました。",
        )
    })?
}

fn elevated_switch_error(error: &AppStateError) -> CommandError {
    match error {
        AppStateError::Elevated(ElevatedCaptureError::Elevation(ElevationError::Cancelled)) => {
            CommandError::new(
                "UAC_CANCELLED",
                "管理者権限モードは有効化されませんでした。",
            )
        }
        _ => CommandError::new(
            "SETTINGS_FAILED",
            format!("管理者権限モードを切り替えられませんでした: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uac_cancel_is_distinguished_from_other_elevation_failures() {
        let cancelled = elevated_switch_error(&AppStateError::Elevated(
            ElevatedCaptureError::Elevation(ElevationError::Cancelled),
        ));
        assert_eq!(cancelled.code, "UAC_CANCELLED");
        assert_eq!(
            cancelled.message,
            "管理者権限モードは有効化されませんでした。"
        );

        let timeout = AppStateError::Elevated(ElevatedCaptureError::HandshakeTimeout);
        let failed = elevated_switch_error(&timeout);
        assert_eq!(failed.code, "SETTINGS_FAILED");
        assert_eq!(
            failed.message,
            format!("管理者権限モードを切り替えられませんでした: {timeout}")
        );
    }
}
