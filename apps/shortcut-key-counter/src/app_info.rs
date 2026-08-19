use serde::Serialize;

pub const AUTHOR_NAME: &str = "KotoriChun";
pub const GITHUB_URL: &str = "https://github.com/KotorinChunChun/ShortcutKeyCounter";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub author: &'static str,
    pub github_url: &'static str,
}

pub fn current() -> AppInfo {
    AppInfo {
        name: "ShortcutKeyCounter",
        version: env!("CARGO_PKG_VERSION"),
        author: AUTHOR_NAME,
        github_url: GITHUB_URL,
    }
}
