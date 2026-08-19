use crate::key_names::key_catalog;
use crate::model::MainInput;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyMappingEntry {
    pub virtual_key: u16,
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MouseKeyMapping {
    pub left_click: bool,
    pub right_click: bool,
    #[serde(default = "default_mouse_mapping_enabled")]
    pub middle_click: bool,
    #[serde(default = "default_mouse_mapping_enabled")]
    pub x1_click: bool,
    #[serde(default = "default_mouse_mapping_enabled")]
    pub x2_click: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,
    #[serde(default = "default_mouse_mapping_enabled")]
    pub wheel_left: bool,
    #[serde(default = "default_mouse_mapping_enabled")]
    pub wheel_right: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyMapping {
    pub version: u32,
    pub keyboard: Vec<KeyMappingEntry>,
    pub mouse: MouseKeyMapping,
}

impl Default for KeyMapping {
    fn default() -> Self {
        Self {
            version: 1,
            keyboard: key_catalog()
                .into_iter()
                .map(|(virtual_key, name)| KeyMappingEntry {
                    virtual_key,
                    name,
                    enabled: true,
                })
                .collect(),
            mouse: MouseKeyMapping {
                left_click: true,
                right_click: true,
                middle_click: true,
                x1_click: true,
                x2_click: true,
                wheel_up: true,
                wheel_down: true,
                wheel_left: true,
                wheel_right: true,
            },
        }
    }
}

impl KeyMapping {
    pub fn allows(&self, input: &MainInput) -> bool {
        match input {
            MainInput::Key(virtual_key) => self
                .keyboard
                .iter()
                .find(|entry| entry.virtual_key == *virtual_key)
                .is_some_and(|entry| entry.enabled),
            MainInput::MouseLeft => self.mouse.left_click,
            MainInput::MouseRight => self.mouse.right_click,
            MainInput::MouseMiddle => self.mouse.middle_click,
            MainInput::MouseX1 => self.mouse.x1_click,
            MainInput::MouseX2 => self.mouse.x2_click,
            MainInput::WheelUp => self.mouse.wheel_up,
            MainInput::WheelDown => self.mouse.wheel_down,
            MainInput::WheelLeft => self.mouse.wheel_left,
            MainInput::WheelRight => self.mouse.wheel_right,
        }
    }

    /// 保存済み設定を現在のキー一覧へマージし、新しい対応キーは既定ONにする。
    pub fn merge_with_current_catalog(saved: Self) -> Self {
        let mut current = Self::default();
        for entry in &mut current.keyboard {
            if let Some(saved_entry) = saved
                .keyboard
                .iter()
                .find(|saved_entry| saved_entry.virtual_key == entry.virtual_key)
            {
                entry.enabled = saved_entry.enabled;
            }
        }
        current.mouse = saved.mouse;
        current
    }
}

fn default_mouse_mapping_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mapping_accepts_supported_inputs() {
        let mapping = KeyMapping::default();
        assert!(mapping.allows(&MainInput::Key(0x43)));
        assert!(mapping.allows(&MainInput::Key(0x70)));
        assert!(mapping.allows(&MainInput::MouseLeft));
        assert!(mapping.allows(&MainInput::MouseMiddle));
    }

    #[test]
    fn disabled_key_is_rejected() {
        let mut mapping = KeyMapping::default();
        mapping
            .keyboard
            .iter_mut()
            .find(|entry| entry.virtual_key == 0x43)
            .expect("C mapping")
            .enabled = false;
        assert!(!mapping.allows(&MainInput::Key(0x43)));
    }

    #[test]
    fn merge_preserves_known_values_and_adds_catalog() {
        let mut saved = KeyMapping::default();
        saved.keyboard.retain(|entry| entry.virtual_key == 0x43);
        saved.keyboard[0].enabled = false;
        let merged = KeyMapping::merge_with_current_catalog(saved);
        assert!(!merged.allows(&MainInput::Key(0x43)));
        assert!(merged.allows(&MainInput::Key(0x56)));
    }
}
