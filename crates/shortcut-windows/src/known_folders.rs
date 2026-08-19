//! Windowsの既知フォルダーに対応するパス解決。

use std::{env, error::Error, ffi::OsString, fmt};

use shortcut_common::paths::AppPaths;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};

/// LocalAppDataの取得に失敗した場合のエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnownFoldersError {
    /// LOCALAPPDATA環境変数が設定されていない。
    LocalAppDataNotSet,
    /// LOCALAPPDATA環境変数が空である。
    LocalAppDataEmpty,
    /// アプリケーションディレクトリの作成に失敗した。
    DirectoryCreation(String),
    /// Downloads Known Folderの解決に失敗した。
    DownloadsUnavailable(String),
    /// Downloads Known Folderのパスが空である。
    DownloadsEmpty,
}

impl fmt::Display for KnownFoldersError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalAppDataNotSet => {
                formatter.write_str("LOCALAPPDATA環境変数が設定されていません")
            }
            Self::LocalAppDataEmpty => formatter.write_str("LOCALAPPDATA環境変数が空です"),
            Self::DirectoryCreation(message) => {
                write!(
                    formatter,
                    "アプリケーションディレクトリを作成できません: {message}"
                )
            }
            Self::DownloadsUnavailable(message) => {
                write!(formatter, "Downloadsフォルダーを解決できません: {message}")
            }
            Self::DownloadsEmpty => formatter.write_str("Downloadsフォルダーのパスが空です"),
        }
    }
}

impl Error for KnownFoldersError {}

/// LOCALAPPDATA配下のアプリケーションパスを取得する。
pub fn local_app_data_paths() -> Result<AppPaths, KnownFoldersError> {
    resolve_local_app_data(env::var_os("LOCALAPPDATA"))
}

/// Windows Known Folder APIで現在ユーザーのDownloadsを取得する。
pub fn downloads_directory() -> Result<std::path::PathBuf, KnownFoldersError> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, Default::default(), None) }
        .map_err(|error| KnownFoldersError::DownloadsUnavailable(error.to_string()))?;
    let result = unsafe { path.to_string() };
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    let path =
        result.map_err(|error| KnownFoldersError::DownloadsUnavailable(error.to_string()))?;
    if path.is_empty() {
        return Err(KnownFoldersError::DownloadsEmpty);
    }
    Ok(std::path::PathBuf::from(path))
}

/// DBとログを置くディレクトリを作成する。
pub fn ensure_app_directories(paths: &AppPaths) -> Result<(), KnownFoldersError> {
    std::fs::create_dir_all(&paths.app_dir)
        .map_err(|error| KnownFoldersError::DirectoryCreation(error.to_string()))?;
    std::fs::create_dir_all(&paths.logs_dir)
        .map_err(|error| KnownFoldersError::DirectoryCreation(error.to_string()))?;
    Ok(())
}

fn resolve_local_app_data(value: Option<OsString>) -> Result<AppPaths, KnownFoldersError> {
    let local_app_data = value.ok_or(KnownFoldersError::LocalAppDataNotSet)?;
    if local_app_data.is_empty() {
        return Err(KnownFoldersError::LocalAppDataEmpty);
    }

    Ok(AppPaths::from_local_app_data(local_app_data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_app_data_paths_have_expected_layout() {
        let paths = resolve_local_app_data(Some(OsString::from(r"C:\Users\test\AppData\Local")))
            .expect("LOCALAPPDATAからパスを構築できること");

        assert!(!paths.app_dir.as_os_str().is_empty());
        assert!(paths.app_dir.ends_with("ShortcutKeyCounter"));
        assert!(paths
            .database_path
            .ends_with(r"ShortcutKeyCounter\ShortcutKeyCounter.db"));
        assert!(paths.logs_dir.ends_with(r"ShortcutKeyCounter\logs"));
    }

    #[test]
    fn missing_local_app_data_is_an_explicit_error() {
        assert_eq!(
            resolve_local_app_data(None),
            Err(KnownFoldersError::LocalAppDataNotSet)
        );
    }

    #[test]
    fn empty_local_app_data_is_an_explicit_error() {
        assert_eq!(
            resolve_local_app_data(Some(OsString::new())),
            Err(KnownFoldersError::LocalAppDataEmpty)
        );
    }

    #[test]
    fn app_directories_are_created() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "shortcut-key-counter-path-test-{}-{unique}",
            std::process::id(),
        ));
        let paths = AppPaths::from_local_app_data(&root);

        ensure_app_directories(&paths).expect("アプリディレクトリを作成できること");

        assert!(paths.app_dir.is_dir());
        assert!(paths.logs_dir.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }
}
