//! キーマッピング、物理キー検索、プロファイル、未保存ガード付き編集セッション。

use shortcut_core::key_mapping::KeyMapping;
use tauri::State;

use super::downloads_directory;
use crate::app_state::{AppState, AppStateError};
use crate::command_error::CommandError;
use crate::key_mapping_state::{self, KeyMappingExportResult};
use crate::mapping_edit_session::{
    MappingEditError, MappingEditSessionSnapshot, MappingEditValuesDto,
};
use crate::mapping_profiles::{self, MappingProfile};
use crate::overlay::config::{KeyboardInputMode, KeyboardLayout};

#[tauri::command]
pub fn begin_key_search(state: State<'_, AppState>) {
    state.key_search.begin();
}

#[tauri::command]
pub fn poll_key_search(state: State<'_, AppState>) -> Option<String> {
    state.key_search.take_result()
}

#[tauri::command]
pub fn cancel_key_search(state: State<'_, AppState>) {
    state.key_search.cancel();
}

#[tauri::command]
pub fn get_key_mapping(state: State<'_, AppState>) -> Result<KeyMapping, CommandError> {
    key_mapping_state::snapshot(&state.key_mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングを読み込めませんでした。",
        )
    })
}

#[tauri::command]
pub fn set_key_mapping(
    state: State<'_, AppState>,
    mapping: KeyMapping,
) -> Result<KeyMapping, CommandError> {
    let path = key_mapping_state::mapping_path(&state.paths.app_dir);
    key_mapping_state::replace_and_save(&path, &state.key_mapping, mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングを保存できませんでした。",
        )
    })
}

#[tauri::command]
pub fn export_key_mapping_json(
    state: State<'_, AppState>,
) -> Result<KeyMappingExportResult, CommandError> {
    let directory = downloads_directory("KEY_MAPPING_FAILED")?;
    key_mapping_state::export_json(&directory, &state.key_mapping).map_err(|_| {
        CommandError::new(
            "KEY_MAPPING_FAILED",
            "キーマッピングJSONを出力できませんでした。",
        )
    })
}

#[tauri::command]
pub fn list_mapping_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<MappingProfile>, CommandError> {
    let snapshot = state.mapping_edit_snapshot().map_err(mapping_edit_error)?;
    mapping_profiles::list(
        &mapping_profiles::path(&state.paths.app_dir),
        &snapshot.draft,
    )
    .map_err(|_| CommandError::new("PROFILE_FAILED", "プロファイルを読み込めませんでした。"))
}

#[tauri::command]
pub fn save_mapping_profile(
    state: State<'_, AppState>,
    name: String,
    values: MappingEditValuesDto,
) -> Result<MappingProfile, CommandError> {
    mapping_profiles::upsert(
        &mapping_profiles::path(&state.paths.app_dir),
        MappingProfile {
            name,
            built_in: false,
            values,
        },
    )
    .map_err(|error| CommandError::new("PROFILE_FAILED", error.to_string()))
}

#[tauri::command]
pub fn delete_mapping_profile(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), CommandError> {
    mapping_profiles::delete(&mapping_profiles::path(&state.paths.app_dir), &name)
        .map_err(|error| CommandError::new("PROFILE_FAILED", error.to_string()))
}

#[tauri::command]
pub fn rename_mapping_profile(
    state: State<'_, AppState>,
    old_name: String,
    new_name: String,
) -> Result<MappingProfile, CommandError> {
    mapping_profiles::rename(
        &mapping_profiles::path(&state.paths.app_dir),
        &old_name,
        &new_name,
    )
    .map_err(|error| CommandError::new("PROFILE_FAILED", error.to_string()))
}

#[tauri::command]
pub fn begin_mapping_edit_session(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.begin_mapping_edit().map_err(mapping_edit_error)
}

#[tauri::command]
pub fn get_mapping_edit_session(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.mapping_edit_snapshot().map_err(mapping_edit_error)
}

#[tauri::command]
pub fn apply_mapping_edit_draft(
    state: State<'_, AppState>,
    draft: MappingEditValuesDto,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state
        .apply_mapping_edit_draft(draft)
        .map_err(mapping_edit_error)
}

#[tauri::command]
pub fn apply_mapping_edit_layout(
    state: State<'_, AppState>,
    draft: MappingEditValuesDto,
    layout: KeyboardLayout,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state
        .apply_mapping_edit_layout(draft, layout)
        .map_err(mapping_edit_error)
}

#[tauri::command]
pub fn apply_mapping_edit_input_mode(
    state: State<'_, AppState>,
    draft: MappingEditValuesDto,
    mode: KeyboardInputMode,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state
        .apply_mapping_edit_input_mode(draft, mode)
        .map_err(mapping_edit_error)
}

#[tauri::command]
pub fn save_mapping_edit_session(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.save_mapping_edit().map_err(mapping_edit_error)
}

#[tauri::command]
pub fn cancel_mapping_edit_session(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.cancel_mapping_edit().map_err(mapping_edit_error)
}

#[tauri::command]
pub fn end_mapping_edit_session(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.end_mapping_edit().map_err(mapping_edit_error)
}

#[tauri::command]
pub fn mapping_edit_session_status(
    state: State<'_, AppState>,
) -> Result<MappingEditSessionSnapshot, CommandError> {
    state.mapping_edit_snapshot().map_err(mapping_edit_error)
}

fn mapping_edit_error(error: AppStateError) -> CommandError {
    let code = match &error {
        AppStateError::MappingEdit(MappingEditError::Dirty) => "MAPPING_EDIT_DIRTY",
        AppStateError::MappingEdit(MappingEditError::Busy) => "MAPPING_EDIT_BUSY",
        AppStateError::MappingEdit(MappingEditError::AlreadyActive) => "MAPPING_EDIT_ACTIVE",
        AppStateError::MappingEdit(MappingEditError::Inactive) => "MAPPING_EDIT_INACTIVE",
        _ => "MAPPING_EDIT_FAILED",
    };
    CommandError::new(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_edit_errors_keep_codes_and_messages() {
        let cases = [
            (MappingEditError::Dirty, "MAPPING_EDIT_DIRTY"),
            (MappingEditError::Busy, "MAPPING_EDIT_BUSY"),
            (MappingEditError::AlreadyActive, "MAPPING_EDIT_ACTIVE"),
            (MappingEditError::Inactive, "MAPPING_EDIT_INACTIVE"),
            (
                MappingEditError::Io(std::io::Error::other("disk")),
                "MAPPING_EDIT_FAILED",
            ),
        ];
        for (error, code) in cases {
            let error = AppStateError::MappingEdit(error);
            let message = error.to_string();
            let converted = mapping_edit_error(error);
            assert_eq!(converted.code, code);
            assert_eq!(converted.message, message);
        }
        assert_eq!(
            mapping_edit_error(AppStateError::Poisoned).code,
            "MAPPING_EDIT_FAILED"
        );
    }
}
