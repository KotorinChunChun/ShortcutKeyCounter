//! UI/Trayへ公開する入力取得経路のhealth state。

use std::sync::{Arc, RwLock};

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSource {
    Local,
    Elevated,
    LocalFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Ok,
    Degraded,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureHealthSnapshot {
    pub source: CaptureSource,
    pub level: HealthLevel,
    pub message: Option<String>,
}

pub type SharedCaptureHealth = Arc<RwLock<CaptureHealthSnapshot>>;

pub fn new_capture_health() -> SharedCaptureHealth {
    Arc::new(RwLock::new(CaptureHealthSnapshot {
        source: CaptureSource::Local,
        level: HealthLevel::Ok,
        message: None,
    }))
}

pub fn update_capture_health(health: &SharedCaptureHealth, snapshot: CaptureHealthSnapshot) {
    if let Ok(mut guard) = health.write() {
        *guard = snapshot;
    }
}

pub fn capture_health_snapshot(health: &SharedCaptureHealth) -> CaptureHealthSnapshot {
    health
        .read()
        .map(|value| value.clone())
        .unwrap_or(CaptureHealthSnapshot {
            source: CaptureSource::LocalFallback,
            level: HealthLevel::Error,
            message: Some("入力取得状態を読み取れません。".to_owned()),
        })
}
