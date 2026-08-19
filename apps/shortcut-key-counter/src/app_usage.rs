use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::Local;
use shortcut_storage::application_usage::add_active_seconds;
use shortcut_storage::db::open_database;
use shortcut_windows::foreground::foreground_executable_name;
use shortcut_windows::user_activity::idle_milliseconds;

use crate::database_environment::SharedDatabaseEnvironment;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(10);
const IDLE_THRESHOLD_MS: u64 = 60_000;

pub type UsageTrackingEnabled = Arc<AtomicBool>;

pub struct AppUsageRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl AppUsageRuntime {
    pub fn start(active_db: SharedDatabaseEnvironment, enabled: UsageTrackingEnabled) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("shortcut-app-usage".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    thread::sleep(SAMPLE_INTERVAL);
                    if thread_stop.load(Ordering::Acquire) || !enabled.load(Ordering::Acquire) {
                        continue;
                    }
                    let Some(idle) = idle_milliseconds() else {
                        continue;
                    };
                    if idle >= IDLE_THRESHOLD_MS {
                        continue;
                    }
                    let Ok(application) = foreground_executable_name() else {
                        continue;
                    };
                    if is_locked_or_credential_shell(&application) {
                        continue;
                    }
                    let path = active_db.current_path();
                    if let Ok(connection) = open_database(&path) {
                        let _ = add_active_seconds(
                            &connection,
                            Local::now().date_naive(),
                            &application,
                            SAMPLE_INTERVAL.as_secs() as i64,
                        );
                    }
                }
            })
            .ok();
        Self { stop, join }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for AppUsageRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_locked_or_credential_shell(application: &str) -> bool {
    matches!(
        application.to_ascii_lowercase().as_str(),
        "logonui.exe" | "lockapp.exe" | "credentialuibroker.exe"
    )
}
