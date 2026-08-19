//! アプリケーションが使用するローカルファイルのパス定義。

use std::path::{Path, PathBuf};

/// LocalAppData配下に作成するアプリディレクトリ名。
pub const APP_DIRECTORY_NAME: &str = "ShortcutKeyCounter";
/// アプリケーションデータベースのファイル名。
pub const DATABASE_FILE_NAME: &str = "ShortcutKeyCounter.db";
/// 検証用SQLiteデータベースのファイル名。
pub const TEST_DATABASE_FILE_NAME: &str = "ShortcutKeyCounter.test.db";
/// ログディレクトリ名。
pub const LOGS_DIRECTORY_NAME: &str = "logs";

/// アプリケーションの永続化先パス。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// アプリケーションデータディレクトリ。
    pub app_dir: PathBuf,
    /// SQLiteデータベースのパス。
    pub database_path: PathBuf,
    /// 検証用SQLiteデータベースのパス。
    pub test_database_path: PathBuf,
    /// ログディレクトリのパス。
    pub logs_dir: PathBuf,
}

impl AppPaths {
    /// LocalAppDataを基準にアプリケーションのパスを構築する。
    pub fn from_local_app_data(local_app_data: impl AsRef<Path>) -> Self {
        let app_dir = local_app_data.as_ref().join(APP_DIRECTORY_NAME);

        Self {
            database_path: app_dir.join(DATABASE_FILE_NAME),
            test_database_path: app_dir.join(TEST_DATABASE_FILE_NAME),
            logs_dir: app_dir.join(LOGS_DIRECTORY_NAME),
            app_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_paths_have_expected_layout() {
        let paths = AppPaths::from_local_app_data(r"C:\Users\test\AppData\Local");

        assert!(!paths.app_dir.as_os_str().is_empty());
        assert!(paths.app_dir.ends_with(APP_DIRECTORY_NAME));
        assert_eq!(paths.database_path, paths.app_dir.join(DATABASE_FILE_NAME));
        assert_eq!(
            paths.test_database_path,
            paths.app_dir.join(TEST_DATABASE_FILE_NAME)
        );
        assert_eq!(paths.logs_dir, paths.app_dir.join(LOGS_DIRECTORY_NAME));
    }
}
