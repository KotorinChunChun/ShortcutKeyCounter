use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::Local;
use serde::Serialize;
use shortcut_core::key_mapping::KeyMapping;
use thiserror::Error;

pub const KEY_MAPPING_FILE_NAME: &str = "ShortcutKeyCounter_key_mapping.json";
pub type SharedKeyMapping = Arc<RwLock<KeyMapping>>;

#[derive(Debug, Error)]
pub enum KeyMappingError {
    #[error("キーマッピングファイルを読み書きできません: {0}")]
    Io(#[from] io::Error),
    #[error("キーマッピングJSONを処理できません: {0}")]
    Json(#[from] serde_json::Error),
    #[error("キーマッピング共有stateが壊れています")]
    Poisoned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KeyMappingExportResult {
    pub path: PathBuf,
}

pub fn mapping_path(app_dir: &Path) -> PathBuf {
    app_dir.join(KEY_MAPPING_FILE_NAME)
}

pub fn load_or_create(path: &Path) -> Result<KeyMapping, KeyMappingError> {
    let mapping = if path.exists() {
        let text = fs::read_to_string(path)?;
        KeyMapping::merge_with_current_catalog(serde_json::from_str(&text)?)
    } else {
        KeyMapping::default()
    };
    save(path, &mapping)?;
    Ok(mapping)
}

pub fn shared(mapping: KeyMapping) -> SharedKeyMapping {
    Arc::new(RwLock::new(mapping))
}

pub fn snapshot(shared: &SharedKeyMapping) -> Result<KeyMapping, KeyMappingError> {
    shared
        .read()
        .map(|value| value.clone())
        .map_err(|_| KeyMappingError::Poisoned)
}

pub fn replace_and_save(
    path: &Path,
    shared: &SharedKeyMapping,
    mapping: KeyMapping,
) -> Result<KeyMapping, KeyMappingError> {
    let normalized = KeyMapping::merge_with_current_catalog(mapping);
    save(path, &normalized)?;
    *shared.write().map_err(|_| KeyMappingError::Poisoned)? = normalized.clone();
    Ok(normalized)
}

pub fn export_json(
    directory: &Path,
    shared: &SharedKeyMapping,
) -> Result<KeyMappingExportResult, KeyMappingError> {
    fs::create_dir_all(directory)?;
    let path = directory.join(format!(
        "ShortcutKeyCounter_key_mapping_{}.json",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    save(&path, &snapshot(shared)?)?;
    Ok(KeyMappingExportResult { path })
}

fn save(path: &Path, mapping: &KeyMapping) -> Result<(), KeyMappingError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(mapping)?;
    fs::write(&temp, data)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("shortcut-key-counter-keymap-{id}"))
            .join(KEY_MAPPING_FILE_NAME)
    }

    #[test]
    fn mapping_roundtrip_is_json_and_persistent() {
        let path = temp_file();
        let mapping = load_or_create(&path).expect("create mapping");
        assert!(path.exists());
        let text = fs::read_to_string(&path).expect("read mapping");
        assert!(text.contains("virtual_key"));
        let loaded = load_or_create(&path).expect("load mapping");
        assert_eq!(mapping, loaded);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
