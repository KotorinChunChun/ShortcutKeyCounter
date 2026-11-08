//! Overlayの表示設定、Preview、表示名・キーポリシー、設定ファイルを開く操作。

use serde::Serialize;
use tauri::{AppHandle, State};

use super::settings_snapshot;
use crate::app_state::{AppState, AppStateError};
use crate::command_api::{OverlaySettingsDto, SettingsSnapshot};
use crate::command_error::CommandError;
use crate::key_policy::KeyPolicyConfig;
use crate::overlay::config::{KeyboardInputMode, KeyboardLayout, OverlayKeyLabels};
use crate::tray::sync_overlay_ui;

#[derive(Clone, Debug, Serialize)]
pub struct OverlayMonitorDto {
    pub id: String,
    pub label: String,
}

#[tauri::command]
pub fn set_overlay_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsSnapshot, CommandError> {
    state.set_overlay_enabled(enabled).map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "オーバーレイ設定を保存できませんでした。",
        )
    })?;
    sync_overlay_ui(&app, enabled);
    settings_snapshot(&state)
}

fn overlay_settings_snapshot(state: &AppState) -> Result<OverlaySettingsDto, CommandError> {
    let enabled = state.overlay_enabled().map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "オーバーレイ設定を読み込めませんでした。",
        )
    })?;
    let policy = state.key_policy_snapshot().map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "キーポリシーを読み込めませんでした。")
    })?;
    Ok(OverlaySettingsDto::from_parts(
        &state.overlay_config(),
        enabled,
        policy.default_single_key,
    ))
}

#[tauri::command]
pub fn get_overlay_settings(
    state: State<'_, AppState>,
) -> Result<OverlaySettingsDto, CommandError> {
    overlay_settings_snapshot(&state)
}

/// 保存前の設定をOverlay threadへPreview専用で渡す。永続ファイル、DB、
/// Realtime Log、統計、通常入力のrepeat stateは変更しない。
#[tauri::command]
pub fn preview_overlay_settings(
    state: State<'_, AppState>,
    settings: OverlaySettingsDto,
    generation: u64,
) -> Result<(), CommandError> {
    let config = settings.apply_to(state.overlay_config());
    state
        .preview_overlay_settings(config, generation)
        .map_err(|error| match error {
            AppStateError::SecurityGateBlocked => CommandError::new(
                "SECURITY_BLOCKED",
                "Security Gateが保護中のため、オーバーレイPreviewを表示できません。",
            ),
            _ => CommandError::new(
                "SETTINGS_FAILED",
                "オーバーレイPreviewを表示できませんでした。",
            ),
        })
}

#[tauri::command]
pub fn get_overlay_monitors() -> Vec<OverlayMonitorDto> {
    shortcut_windows::overlay_window::available_monitors()
        .into_iter()
        .map(|monitor| OverlayMonitorDto {
            label: monitor.label,
            id: monitor.id,
        })
        .collect()
}

#[tauri::command]
pub fn preview_overlay_monitor(state: State<'_, AppState>, monitor_id: String, number: u32) {
    state
        .overlay
        .preview_monitor(monitor_id, format!("モニター {number}"));
}

#[tauri::command]
pub fn set_overlay_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: OverlaySettingsDto,
) -> Result<OverlaySettingsDto, CommandError> {
    let config = settings.apply_to(state.overlay_config());
    state
        .apply_overlay_settings(config, settings.enabled, settings.default_single_key_action)
        .map_err(|_| {
            CommandError::new(
                "SETTINGS_FAILED",
                "オーバーレイ設定を保存できませんでした。",
            )
        })?;
    sync_overlay_ui(&app, settings.enabled);
    overlay_settings_snapshot(&state)
}

#[tauri::command]
pub fn reset_overlay_appearance(
    state: State<'_, AppState>,
) -> Result<OverlaySettingsDto, CommandError> {
    let config = state.reset_overlay_appearance().map_err(|_| {
        CommandError::new(
            "SETTINGS_FAILED",
            "オーバーレイ外観設定を初期値へ戻せませんでした。",
        )
    })?;
    let enabled = state
        .overlay_enabled()
        .map_err(|_| CommandError::new("SETTINGS_FAILED", "設定を読み込めませんでした。"))?;
    let policy = state.key_policy_snapshot().map_err(|_| {
        CommandError::new("SETTINGS_FAILED", "キーポリシーを読み込めませんでした。")
    })?;
    Ok(OverlaySettingsDto::from_parts(
        &config,
        enabled,
        policy.default_single_key,
    ))
}

#[tauri::command]
pub fn get_overlay_key_labels(state: State<'_, AppState>) -> OverlayKeyLabels {
    state.overlay_labels()
}

#[tauri::command]
pub fn set_overlay_key_labels(
    state: State<'_, AppState>,
    labels: OverlayKeyLabels,
) -> Result<OverlayKeyLabels, CommandError> {
    state
        .replace_overlay_labels(labels)
        .map_err(|_| CommandError::new("KEY_LABELS_FAILED", "表示名を保存できませんでした。"))
}

#[tauri::command]
pub fn set_overlay_keyboard_layout(
    state: State<'_, AppState>,
    layout: KeyboardLayout,
) -> Result<OverlayKeyLabels, CommandError> {
    state.replace_overlay_keyboard_layout(layout).map_err(|_| {
        CommandError::new(
            "KEY_LABELS_FAILED",
            "キーボード種類を保存できませんでした。",
        )
    })
}

#[tauri::command]
pub fn set_overlay_keyboard_input_mode(
    state: State<'_, AppState>,
    mode: KeyboardInputMode,
) -> Result<OverlayKeyLabels, CommandError> {
    state
        .replace_overlay_keyboard_input_mode(mode)
        .map_err(|_| {
            CommandError::new(
                "KEY_LABELS_FAILED",
                "キーボード入力モードを保存できませんでした。",
            )
        })
}

#[tauri::command]
pub fn get_overlay_key_policy(state: State<'_, AppState>) -> Result<KeyPolicyConfig, CommandError> {
    state
        .key_policy_snapshot()
        .map_err(|_| CommandError::new("KEY_POLICY_FAILED", "キーポリシーを読み込めませんでした。"))
}

#[tauri::command]
pub fn set_overlay_key_policy(
    state: State<'_, AppState>,
    policy: KeyPolicyConfig,
) -> Result<KeyPolicyConfig, CommandError> {
    state
        .replace_key_policy(policy)
        .map_err(|_| CommandError::new("KEY_POLICY_FAILED", "キーポリシーを保存できませんでした。"))
}

#[tauri::command]
pub fn reload_overlay_assets(state: State<'_, AppState>) {
    state.reload_overlay_assets();
}

#[tauri::command]
pub fn preview_overlay_sample(state: State<'_, AppState>) {
    state.overlay.preview_sample();
}

#[tauri::command]
pub fn open_overlay_labels(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.labels_path())
        .map_err(|_| CommandError::new("OPEN_FILE_FAILED", "表示名JSONを開けませんでした。"))
}

#[tauri::command]
pub fn open_overlay_theme(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.theme_path()).map_err(|_| {
        CommandError::new(
            "OPEN_FILE_FAILED",
            "オーバーレイ設定JSONを開けませんでした。",
        )
    })
}

#[tauri::command]
pub fn open_overlay_policy(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.policy_path())
        .map_err(|_| CommandError::new("OPEN_FILE_FAILED", "キーポリシーJSONを開けませんでした。"))
}

#[tauri::command]
pub fn open_overlay_design_folder(state: State<'_, AppState>) -> Result<(), CommandError> {
    shortcut_windows::shell_open::open_path(state.overlay.design_directory()).map_err(|_| {
        CommandError::new(
            "OPEN_FOLDER_FAILED",
            "オーバーレイデザインフォルダーを開けませんでした。",
        )
    })
}
