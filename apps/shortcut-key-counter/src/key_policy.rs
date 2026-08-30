use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::overlay::event::OverlayInputKind;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Ignore,
    #[serde(rename = "continue_display", alias = "display_only")]
    ContinueDisplay,
    DisplayBreak,
    #[serde(rename = "record_and_display_break", alias = "display_and_record")]
    RecordAndDisplayBreak,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyDecision {
    pub display: bool,
    pub record: bool,
    pub break_row: bool,
}

impl From<KeyAction> for KeyDecision {
    fn from(action: KeyAction) -> Self {
        match action {
            KeyAction::Ignore => Self {
                display: false,
                record: false,
                break_row: true,
            },
            KeyAction::ContinueDisplay => Self {
                display: true,
                record: false,
                break_row: false,
            },
            KeyAction::DisplayBreak => Self {
                display: true,
                record: false,
                break_row: true,
            },
            KeyAction::RecordAndDisplayBreak => Self {
                display: true,
                record: true,
                break_row: true,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyPolicyConfig {
    pub version: u32,
    pub default_single_key: KeyAction,
    pub default_shortcut: KeyAction,
    #[serde(default)]
    pub overrides: HashMap<String, KeyAction>,
}

impl Default for KeyPolicyConfig {
    fn default() -> Self {
        Self {
            version: 2,
            default_single_key: KeyAction::ContinueDisplay,
            default_shortcut: KeyAction::RecordAndDisplayBreak,
            overrides: HashMap::new(),
        }
    }
}

impl KeyPolicyConfig {
    pub fn resolve(&self, identity: &str, kind: OverlayInputKind) -> KeyAction {
        if let Some(action) = self.overrides.get(identity) {
            return *action;
        }
        match kind {
            OverlayInputKind::SingleKey => self.default_single_key,
            OverlayInputKind::Shortcut => self.default_shortcut,
        }
    }

    pub fn sanitize(mut self) -> Self {
        self.version = 2;
        // Security hardening: settings must never be able to opt an unmodified
        // single input into persistence. Runtime enforcement remains the final
        // authority, but sanitizing the stored policy also prevents misleading UI.
        self.overrides.retain(|identity, action| {
            if !identity.contains('+') && matches!(action, KeyAction::RecordAndDisplayBreak) {
                *action = KeyAction::ContinueDisplay;
            }
            true
        });
        if matches!(self.default_single_key, KeyAction::RecordAndDisplayBreak) {
            self.default_single_key = KeyAction::ContinueDisplay;
        }
        self
    }
}

pub type SharedKeyPolicy = Arc<RwLock<KeyPolicyConfig>>;

#[derive(Debug, Error)]
pub enum KeyPolicyError {
    #[error("キーポリシーを読み込めません: {0}")]
    Read(#[source] std::io::Error),
    #[error("キーポリシーを書き込めません: {0}")]
    Write(#[source] std::io::Error),
    #[error("キーポリシーJSONが不正です: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("キーポリシー共有stateが壊れています")]
    Poisoned,
}

pub fn shared(config: KeyPolicyConfig) -> SharedKeyPolicy {
    Arc::new(RwLock::new(config))
}

pub fn load_or_create(path: &Path) -> Result<KeyPolicyConfig, KeyPolicyError> {
    if !path.exists() {
        let config = KeyPolicyConfig::default();
        save_atomic(path, &config)?;
        return Ok(config);
    }
    let text = fs::read_to_string(path).map_err(KeyPolicyError::Read)?;
    let config = serde_json::from_str::<KeyPolicyConfig>(&text)
        .map_err(KeyPolicyError::Parse)?
        .sanitize();
    Ok(config)
}

pub fn snapshot(shared: &SharedKeyPolicy) -> Result<KeyPolicyConfig, KeyPolicyError> {
    shared
        .read()
        .map(|value| value.clone())
        .map_err(|_| KeyPolicyError::Poisoned)
}

pub fn replace_and_save(
    path: &Path,
    shared: &SharedKeyPolicy,
    config: KeyPolicyConfig,
) -> Result<KeyPolicyConfig, KeyPolicyError> {
    let normalized = config.sanitize();
    save_atomic(path, &normalized)?;
    *shared.write().map_err(|_| KeyPolicyError::Poisoned)? = normalized.clone();
    Ok(normalized)
}

pub fn save_atomic(path: &Path, config: &KeyPolicyConfig) -> Result<(), KeyPolicyError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(KeyPolicyError::Write)?;
    }
    let text =
        serde_json::to_string_pretty(&config.clone().sanitize()).map_err(KeyPolicyError::Parse)?;
    let temp = path.with_extension("json.tmp");
    let result = (|| {
        let mut file = fs::File::create(&temp).map_err(KeyPolicyError::Write)?;
        file.write_all(format!("{text}\n").as_bytes())
            .map_err(KeyPolicyError::Write)?;
        file.flush().map_err(KeyPolicyError::Write)?;
        file.sync_all().map_err(KeyPolicyError::Write)?;
        if fs::rename(&temp, path).is_err() {
            if path.exists() {
                fs::remove_file(path).map_err(KeyPolicyError::Write)?;
            }
            fs::rename(&temp, path).map_err(KeyPolicyError::Write)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::event::OverlayInputKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn defaults_separate_single_key_display_from_shortcut_recording() {
        let config = KeyPolicyConfig::default();
        assert_eq!(
            config.resolve("A", OverlayInputKind::SingleKey),
            KeyAction::ContinueDisplay
        );
        assert_eq!(
            config.resolve("Ctrl+C", OverlayInputKind::Shortcut),
            KeyAction::RecordAndDisplayBreak
        );
    }

    #[test]
    fn record_decision_always_includes_display() {
        assert!(KeyDecision::from(KeyAction::RecordAndDisplayBreak).display);
        assert!(KeyDecision::from(KeyAction::RecordAndDisplayBreak).record);
        assert!(KeyDecision::from(KeyAction::RecordAndDisplayBreak).break_row);
        assert!(!KeyDecision::from(KeyAction::ContinueDisplay).break_row);
        assert!(!KeyDecision::from(KeyAction::Ignore).display);
    }

    #[test]
    fn missing_policy_file_is_created_with_safe_defaults() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("shortcut-key-policy-{unique}"));
        let path = directory.join("overlay_key_policy.json");
        let policy = load_or_create(&path).expect("create policy");
        assert_eq!(policy.default_single_key, KeyAction::ContinueDisplay);
        assert_eq!(policy.default_shortcut, KeyAction::RecordAndDisplayBreak);
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_policy_actions_migrate_to_new_matrix_actions() {
        let config: KeyPolicyConfig = serde_json::from_str(
            r#"{"version":1,"default_single_key":"display_only","default_shortcut":"display_and_record","overrides":{}}"#,
        )
        .expect("legacy policy");
        assert_eq!(config.default_single_key, KeyAction::ContinueDisplay);
        assert_eq!(config.default_shortcut, KeyAction::RecordAndDisplayBreak);
        assert_eq!(config.sanitize().version, 2);
    }
}
