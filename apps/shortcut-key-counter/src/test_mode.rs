//! Debugビルドだけで有効になる隔離試験モード。Releaseビルドでは常に無効。
//!
//! - `SHORTCUT_KEY_COUNTER_TEST_LOCALAPPDATA`: 保存先・出力先を指定ディレクトリへ隔離し、
//!   入力Hook、自動起動同期、Overlay、本体管理窓口を使わない。単一起動も通常版とは
//!   別の名前空間で判定し、起動中の通常版を前面へ呼び出さない。
//! - `SHORTCUT_KEY_COUNTER_TEST_BACKGROUND_UI=1`: 上記に加えて、UIを非表示・非アクティブ・
//!   タスクバー非表示で生成する。CLIからDevTools Protocol経由で操作し、
//!   デスクトップの最前面には出さない。

use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub const LOCALAPPDATA_ENV: &str = "SHORTCUT_KEY_COUNTER_TEST_LOCALAPPDATA";
pub const BACKGROUND_UI_ENV: &str = "SHORTCUT_KEY_COUNTER_TEST_BACKGROUND_UI";

/// 隔離試験の保存先root。Releaseビルドと未指定時は`None`。
pub fn isolated_root() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var_os(LOCALAPPDATA_ENV).map(PathBuf::from)
}

pub fn is_isolated() -> bool {
    isolated_root().is_some()
}

/// UIを前面に出さずに生成する試験モードか。
pub fn background_ui() -> bool {
    background_ui_enabled(
        is_isolated(),
        std::env::var_os(BACKGROUND_UI_ENV).as_deref(),
    )
}

fn background_ui_enabled(isolated: bool, value: Option<&OsStr>) -> bool {
    // 隔離されていない通常起動では、環境変数があってもUIを隠さない。
    isolated && value.is_some_and(|value| value == "1")
}

/// 隔離root単位の単一起動名前空間。同じrootの二重起動だけを1つにまとめる。
pub fn single_instance_namespace(root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    root.to_string_lossy().to_lowercase().hash(&mut hasher);
    format!("Test.{:016x}", hasher.finish())
}

/// 隔離試験中のCSV・バックアップ・JSON出力先。実際のDownloadsへは書かない。
pub fn isolated_downloads_directory() -> Option<PathBuf> {
    isolated_root().map(|root| root.join("Downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_ui_requires_isolation_and_explicit_one() {
        assert!(background_ui_enabled(true, Some(OsStr::new("1"))));
        assert!(!background_ui_enabled(false, Some(OsStr::new("1"))));
        assert!(!background_ui_enabled(true, None));
        assert!(!background_ui_enabled(true, Some(OsStr::new("true"))));
        assert!(!background_ui_enabled(true, Some(OsStr::new("0"))));
    }

    #[test]
    fn single_instance_namespace_is_stable_per_root_and_name_safe() {
        let first = single_instance_namespace(Path::new(r"C:\Temp\skc-a"));
        assert_eq!(
            first,
            single_instance_namespace(Path::new(r"c:\temp\SKC-A"))
        );
        assert_ne!(
            first,
            single_instance_namespace(Path::new(r"C:\Temp\skc-b"))
        );
        assert!(first.starts_with("Test."));
        assert!(!first.contains(['\\', '/']));
    }
}
