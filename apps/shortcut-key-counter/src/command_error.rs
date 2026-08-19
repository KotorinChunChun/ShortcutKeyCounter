//! Tauri境界でUIへ返す安全なエラーDTO。

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn db() -> Self {
        Self::new("DB_QUERY_FAILED", "データベース処理に失敗しました。")
    }

    pub fn state() -> Self {
        Self::new("STATE_UNAVAILABLE", "アプリ内部状態を取得できません。")
    }
}
