//! キーマッピング画面の保存前編集セッション。
//!
//! 編集中の値は共有 runtime に反映するが、保存成功まではJSONへ書き込まない。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use shortcut_core::key_mapping::KeyMapping;
use thiserror::Error;

use crate::key_policy::KeyPolicyConfig;
use crate::overlay::config::{OverlayKeyLabels, OverlayPaths};

static FILE_TRANSACTION_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingEditValues {
    pub mapping: KeyMapping,
    pub policy: KeyPolicyConfig,
    pub labels: OverlayKeyLabels,
}

impl MappingEditValues {
    pub fn normalize(self) -> Self {
        Self {
            mapping: KeyMapping::merge_with_current_catalog(self.mapping),
            policy: self.policy.sanitize(),
            labels: OverlayKeyLabels::merge_with_defaults(self.labels),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MappingEditSessionSnapshot {
    pub saved: MappingEditValuesDto,
    pub draft: MappingEditValuesDto,
    pub active: bool,
    pub busy: bool,
    pub dirty: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MappingEditValuesDto {
    pub mapping: KeyMapping,
    pub policy: KeyPolicyConfig,
    pub labels: OverlayKeyLabels,
}

impl From<&MappingEditValues> for MappingEditValuesDto {
    fn from(value: &MappingEditValues) -> Self {
        Self {
            mapping: value.mapping.clone(),
            policy: value.policy.clone(),
            labels: value.labels.clone(),
        }
    }
}

impl From<MappingEditValuesDto> for MappingEditValues {
    fn from(value: MappingEditValuesDto) -> Self {
        Self {
            mapping: value.mapping,
            policy: value.policy,
            labels: value.labels,
        }
    }
}

#[derive(Debug, Error)]
pub enum MappingEditError {
    #[error("キーマッピング編集セッションが開始されていません")]
    Inactive,
    #[error("キーマッピング編集セッションは既に開始されています")]
    AlreadyActive,
    #[error("キーマッピング編集セッションは処理中です")]
    Busy,
    #[error("未保存のキーマッピング変更があります")]
    Dirty,
    #[error("キーマッピング設定の保存に失敗しました: {0}")]
    Io(#[from] io::Error),
    #[error("キーマッピング設定のJSON変換に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
}

/// 一度に1つだけ開ける編集セッション。`saved` は最後の保存成功値、`draft` はruntime値。
#[derive(Clone, Debug, Default)]
pub struct MappingEditSession {
    saved: Option<MappingEditValues>,
    draft: Option<MappingEditValues>,
    busy: bool,
}

impl MappingEditSession {
    pub fn begin(
        &mut self,
        values: MappingEditValues,
    ) -> Result<MappingEditSessionSnapshot, MappingEditError> {
        if self.busy {
            return Err(MappingEditError::Busy);
        }
        if self.saved.is_some() {
            return Err(MappingEditError::AlreadyActive);
        }
        let values = values.normalize();
        self.saved = Some(values.clone());
        self.draft = Some(values);
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> MappingEditSessionSnapshot {
        let empty = MappingEditValues {
            mapping: KeyMapping::default(),
            policy: KeyPolicyConfig::default(),
            labels: OverlayKeyLabels::default(),
        };
        let saved = self.saved.as_ref().unwrap_or(&empty);
        let draft = self.draft.as_ref().unwrap_or(saved);
        MappingEditSessionSnapshot {
            saved: saved.into(),
            draft: draft.into(),
            active: self.saved.is_some(),
            busy: self.busy,
            dirty: self.is_dirty(),
        }
    }

    pub fn draft(&self) -> Result<MappingEditValues, MappingEditError> {
        self.draft.clone().ok_or(MappingEditError::Inactive)
    }

    pub fn saved(&self) -> Result<MappingEditValues, MappingEditError> {
        self.saved.clone().ok_or(MappingEditError::Inactive)
    }

    pub fn apply_draft(
        &mut self,
        values: MappingEditValues,
    ) -> Result<MappingEditValues, MappingEditError> {
        if self.busy {
            return Err(MappingEditError::Busy);
        }
        if self.saved.is_none() {
            return Err(MappingEditError::Inactive);
        }
        let values = values.normalize();
        self.draft = Some(values.clone());
        Ok(values)
    }

    pub fn begin_busy(&mut self) -> Result<(), MappingEditError> {
        if self.saved.is_none() {
            return Err(MappingEditError::Inactive);
        }
        if self.busy {
            return Err(MappingEditError::Busy);
        }
        self.busy = true;
        Ok(())
    }

    pub fn end_busy(&mut self) {
        self.busy = false;
    }

    pub fn mark_saved(&mut self) -> Result<(), MappingEditError> {
        if !self.busy {
            return Err(MappingEditError::Busy);
        }
        self.saved = Some(self.draft()?);
        Ok(())
    }

    pub fn restore_saved(&mut self) -> Result<MappingEditValues, MappingEditError> {
        if !self.busy {
            return Err(MappingEditError::Busy);
        }
        let saved = self.saved()?;
        self.draft = Some(saved.clone());
        Ok(saved)
    }

    pub fn finish(&mut self) -> Result<(), MappingEditError> {
        if self.busy {
            return Err(MappingEditError::Busy);
        }
        if self.is_dirty() {
            return Err(MappingEditError::Dirty);
        }
        self.saved = None;
        self.draft = None;
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        matches!((&self.saved, &self.draft), (Some(saved), Some(draft)) if saved != draft)
    }
}

#[derive(Clone, Copy)]
struct TransactionFile<'a> {
    path: &'a Path,
    bytes: &'a [u8],
}

/// 3つの設定JSONを同じトランザクションとして置換する。
/// 途中で失敗した場合は、存在していたファイルをbackupから復元する。
pub fn save_all_atomic(
    mapping_path: &Path,
    overlay_paths: &OverlayPaths,
    values: &MappingEditValues,
) -> Result<(), MappingEditError> {
    save_all_atomic_with_failure(mapping_path, overlay_paths, values, None)
}

fn save_all_atomic_with_failure(
    mapping_path: &Path,
    overlay_paths: &OverlayPaths,
    values: &MappingEditValues,
    fail_after: Option<usize>,
) -> Result<(), MappingEditError> {
    let values = values.clone().normalize();
    let mapping = serde_json::to_vec_pretty(&values.mapping)?;
    let policy = serde_json::to_vec_pretty(&values.policy)?;
    let labels = serde_json::to_vec_pretty(&values.labels)?;
    let files = [
        TransactionFile {
            path: mapping_path,
            bytes: &mapping,
        },
        TransactionFile {
            path: &overlay_paths.policy_json,
            bytes: &policy,
        },
        TransactionFile {
            path: &overlay_paths.labels_json,
            bytes: &labels,
        },
    ];
    let id = transaction_id();
    let temps: Vec<PathBuf> = files
        .iter()
        .map(|file| {
            file.path.with_file_name(format!(
                "{}.mapping-edit-{id}.tmp",
                file.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("settings")
            ))
        })
        .collect();
    let backups: Vec<PathBuf> = files
        .iter()
        .map(|file| {
            file.path.with_file_name(format!(
                "{}.mapping-edit-{id}.bak",
                file.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("settings")
            ))
        })
        .collect();
    let mut backed_up = vec![false; files.len()];
    let mut replaced = vec![false; files.len()];

    let result = (|| {
        for (index, (file, temp)) in files.iter().zip(&temps).enumerate() {
            if let Some(parent) = file.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut handle = fs::File::create(temp)?;
            use std::io::Write;
            handle.write_all(file.bytes)?;
            handle.write_all(b"\n")?;
            handle.sync_all()?;
            if file.path.exists() {
                fs::copy(file.path, &backups[index])?;
                backed_up[index] = true;
            }
        }
        for (index, (file, temp)) in files.iter().zip(&temps).enumerate() {
            if fail_after == Some(index) {
                return Err(io::Error::other("テスト用の保存途中失敗").into());
            }
            if file.path.exists() {
                fs::remove_file(file.path)?;
            }
            fs::rename(temp, file.path)?;
            replaced[index] = true;
        }
        Ok(())
    })();

    if result.is_err() {
        for (index, file) in files.iter().enumerate() {
            if replaced[index] {
                let _ = fs::remove_file(file.path);
            }
            if backed_up[index] {
                let _ = fs::copy(&backups[index], file.path);
            }
        }
    }
    for path in temps.iter().chain(&backups) {
        let _ = fs::remove_file(path);
    }
    result
}

fn transaction_id() -> String {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{clock}-{}",
        FILE_TRANSACTION_ID.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> (PathBuf, OverlayPaths) {
        let root = std::env::temp_dir().join(format!("shortcut-mapping-edit-{}", transaction_id()));
        let overlay = OverlayPaths {
            theme_json: root.join("overlay-theme.json"),
            labels_json: root.join("overlay_key_labels.json"),
            policy_json: root.join("overlay_key_policy.json"),
            design_directory: root.join("overlay"),
        };
        (root.join("mapping.json"), overlay)
    }

    fn values() -> MappingEditValues {
        MappingEditValues {
            mapping: KeyMapping::default(),
            policy: KeyPolicyConfig::default(),
            labels: OverlayKeyLabels::default(),
        }
    }

    #[test]
    fn draft_is_not_persisted_and_returning_to_saved_clears_dirty() {
        let mut session = MappingEditSession::default();
        session.begin(values()).expect("begin");
        let mut draft = values();
        draft.mapping.mouse.left_click = false;
        session.apply_draft(draft.clone()).expect("apply");
        assert!(session.is_dirty());
        draft.mapping.mouse.left_click = true;
        session.apply_draft(draft).expect("restore draft");
        assert!(!session.is_dirty());
    }

    #[test]
    fn save_all_atomic_rolls_back_when_second_file_fails() {
        let (mapping, paths) = paths();
        let values = values();
        save_all_atomic(&mapping, &paths, &values).expect("initial save");
        let old_mapping = fs::read(&mapping).expect("mapping");
        let old_policy = fs::read(&paths.policy_json).expect("policy");
        let mut changed = values.clone();
        changed.mapping.mouse.left_click = false;
        save_all_atomic_with_failure(&mapping, &paths, &changed, Some(1)).expect_err("must fail");
        assert_eq!(fs::read(mapping).expect("mapping restored"), old_mapping);
        assert_eq!(
            fs::read(paths.policy_json).expect("policy restored"),
            old_policy
        );
        assert_eq!(
            fs::read(paths.labels_json).expect("labels restored"),
            serde_json::to_vec_pretty(&values.labels)
                .unwrap()
                .into_iter()
                .chain([b'\n'])
                .collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(paths.theme_json.parent().unwrap());
    }

    #[test]
    fn values_are_normalized_before_session_comparison() {
        let mut session = MappingEditSession::default();
        session.begin(values()).expect("begin");
        let mut draft = values();
        draft.policy.version = 0;
        session.apply_draft(draft).expect("apply");
        assert!(!session.is_dirty());
    }
}
