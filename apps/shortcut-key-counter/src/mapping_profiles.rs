use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mapping_edit_session::MappingEditValuesDto;
use crate::overlay::config::{KeyboardInputMode, KeyboardLayout, OverlayKeyLabels};

const BUILT_IN_NAMES: [&str; 4] = [
    "日本語109 ローマ字",
    "日本語109 かな入力",
    "英語104",
    "ISO 105",
];

fn is_built_in_name(name: &str) -> bool {
    BUILT_IN_NAMES.contains(&name)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MappingProfile {
    pub name: String,
    #[serde(default)]
    pub built_in: bool,
    pub values: MappingEditValuesDto,
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("プロファイルファイルを読み込めません: {0}")]
    Read(#[from] std::io::Error),
    #[error("プロファイルJSONが不正です: {0}")]
    Json(#[from] serde_json::Error),
    #[error("プロファイル名が不正です")]
    InvalidName,
    #[error("組み込みプロファイルは変更できません")]
    BuiltIn,
}

pub fn path(app_dir: &Path) -> PathBuf {
    app_dir.join("ShortcutKeyCounter_key_profiles.json")
}

fn load_custom(path: &Path) -> Result<Vec<MappingProfile>, ProfileError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn save_custom(path: &Path, profiles: &[MappingProfile]) -> Result<(), ProfileError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(profiles)?),
    )?;
    Ok(())
}

pub fn list(
    path: &Path,
    current: &MappingEditValuesDto,
) -> Result<Vec<MappingProfile>, ProfileError> {
    let mut profiles = vec![
        MappingProfile {
            name: "日本語109 ローマ字".into(),
            built_in: true,
            values: MappingEditValuesDto {
                labels: OverlayKeyLabels::for_layout_mode(
                    KeyboardLayout::Japanese109,
                    KeyboardInputMode::Romaji,
                ),
                mapping: current.mapping.clone(),
                policy: current.policy.clone(),
            },
        },
        MappingProfile {
            name: "日本語109 かな入力".into(),
            built_in: true,
            values: MappingEditValuesDto {
                labels: OverlayKeyLabels::for_layout_mode(
                    KeyboardLayout::Japanese109,
                    KeyboardInputMode::Kana,
                ),
                mapping: current.mapping.clone(),
                policy: current.policy.clone(),
            },
        },
        MappingProfile {
            name: "英語104".into(),
            built_in: true,
            values: MappingEditValuesDto {
                labels: OverlayKeyLabels::for_layout(KeyboardLayout::Us104),
                mapping: current.mapping.clone(),
                policy: current.policy.clone(),
            },
        },
        MappingProfile {
            name: "ISO 105".into(),
            built_in: true,
            values: MappingEditValuesDto {
                labels: OverlayKeyLabels::for_layout(KeyboardLayout::Iso105),
                mapping: current.mapping.clone(),
                policy: current.policy.clone(),
            },
        },
    ];
    profiles.extend(load_custom(path)?);
    Ok(profiles)
}

pub fn upsert(path: &Path, mut profile: MappingProfile) -> Result<MappingProfile, ProfileError> {
    let name = profile.name.trim().to_owned();
    if name.is_empty() || name.len() > 80 {
        return Err(ProfileError::InvalidName);
    }
    if is_built_in_name(&name) {
        return Err(ProfileError::BuiltIn);
    }
    profile.name = name;
    profile.built_in = false;
    let mut profiles = load_custom(path)?;
    profiles.retain(|item| item.name != profile.name);
    profiles.push(profile.clone());
    save_custom(path, &profiles)?;
    Ok(profile)
}

pub fn delete(path: &Path, name: &str) -> Result<(), ProfileError> {
    if name.trim().is_empty() {
        return Err(ProfileError::InvalidName);
    }
    if is_built_in_name(name) {
        return Err(ProfileError::BuiltIn);
    }
    let mut profiles = load_custom(path)?;
    if !profiles.iter().any(|item| item.name == name) {
        return Ok(());
    }
    profiles.retain(|item| item.name != name);
    save_custom(path, &profiles)
}

pub fn rename(path: &Path, old_name: &str, new_name: &str) -> Result<MappingProfile, ProfileError> {
    let mut profiles = load_custom(path)?;
    let Some(profile) = profiles.iter_mut().find(|item| item.name == old_name) else {
        return Err(ProfileError::InvalidName);
    };
    let new_name = new_name.trim().to_owned();
    if new_name.is_empty() || new_name.len() > 80 {
        return Err(ProfileError::InvalidName);
    }
    if is_built_in_name(old_name) || is_built_in_name(&new_name) {
        return Err(ProfileError::BuiltIn);
    }
    profile.name = new_name;
    let result = profile.clone();
    save_custom(path, &profiles)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_policy::KeyPolicyConfig;
    use shortcut_core::key_mapping::KeyMapping;

    fn values() -> MappingEditValuesDto {
        MappingEditValuesDto {
            mapping: KeyMapping::default(),
            policy: KeyPolicyConfig::default(),
            labels: OverlayKeyLabels::default(),
        }
    }

    #[test]
    fn custom_profile_round_trip_and_rename() {
        let dir = std::env::temp_dir().join(format!("shortcut-profiles-{}", std::process::id()));
        let file = path(&dir);
        let saved = upsert(
            &file,
            MappingProfile {
                name: "仕事用".into(),
                built_in: false,
                values: values(),
            },
        )
        .expect("save");
        assert_eq!(saved.name, "仕事用");
        assert!(list(&file, &values())
            .expect("list")
            .iter()
            .any(|p| p.name == "仕事用"));
        assert_eq!(
            rename(&file, "仕事用", "自宅用").expect("rename").name,
            "自宅用"
        );
        delete(&file, "自宅用").expect("delete");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn builtin_profiles_include_iso105() {
        let profiles =
            list(std::path::Path::new("missing-iso-profiles.json"), &values()).expect("list");
        let iso = profiles
            .iter()
            .find(|profile| profile.name == "ISO 105")
            .expect("iso profile");
        assert!(iso.built_in);
        assert_eq!(iso.values.labels.keyboard_layout, KeyboardLayout::Iso105);
        assert_eq!(iso.values.labels.display("Oem102"), "<>");
    }

    #[test]
    fn builtin_names_cannot_be_saved_deleted_or_renamed() {
        let dir =
            std::env::temp_dir().join(format!("shortcut-builtin-profiles-{}", std::process::id()));
        let file = path(&dir);

        for name in BUILT_IN_NAMES {
            assert!(matches!(
                upsert(
                    &file,
                    MappingProfile {
                        name: name.to_owned(),
                        built_in: false,
                        values: values(),
                    }
                ),
                Err(ProfileError::BuiltIn)
            ));
            assert!(matches!(delete(&file, name), Err(ProfileError::BuiltIn)));
        }

        upsert(
            &file,
            MappingProfile {
                name: "仕事用".to_owned(),
                built_in: false,
                values: values(),
            },
        )
        .expect("save custom profile");
        assert!(matches!(
            rename(&file, "仕事用", "ISO 105"),
            Err(ProfileError::BuiltIn)
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
