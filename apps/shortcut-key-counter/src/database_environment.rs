use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use shortcut_common::paths::AppPaths;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEnvironment {
    #[default]
    Production,
    Test,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatabaseEnvironmentSnapshot {
    pub environment: DatabaseEnvironment,
    pub display_name: String,
    pub path: String,
    pub is_test: bool,
}

#[derive(Clone)]
pub struct SharedDatabaseEnvironment {
    inner: Arc<RwLock<DatabaseEnvironment>>,
    production_path: PathBuf,
    test_path: PathBuf,
}

impl SharedDatabaseEnvironment {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            inner: Arc::new(RwLock::new(DatabaseEnvironment::Production)),
            production_path: paths.database_path.clone(),
            test_path: paths.test_database_path.clone(),
        }
    }

    pub fn current(&self) -> DatabaseEnvironment {
        self.inner
            .read()
            .map(|v| *v)
            .unwrap_or(DatabaseEnvironment::Production)
    }

    pub fn set(&self, environment: DatabaseEnvironment) {
        if let Ok(mut value) = self.inner.write() {
            *value = environment;
        }
    }

    pub fn path_for(&self, environment: DatabaseEnvironment) -> PathBuf {
        match environment {
            DatabaseEnvironment::Production => self.production_path.clone(),
            DatabaseEnvironment::Test => self.test_path.clone(),
        }
    }

    pub fn current_path(&self) -> PathBuf {
        self.path_for(self.current())
    }

    pub fn snapshot(&self) -> DatabaseEnvironmentSnapshot {
        let environment = self.current();
        DatabaseEnvironmentSnapshot {
            environment,
            display_name: match environment {
                DatabaseEnvironment::Production => "本番DB".to_owned(),
                DatabaseEnvironment::Test => "検証DB".to_owned(),
            },
            path: self.path_for(environment).to_string_lossy().into_owned(),
            is_test: environment == DatabaseEnvironment::Test,
        }
    }
}
