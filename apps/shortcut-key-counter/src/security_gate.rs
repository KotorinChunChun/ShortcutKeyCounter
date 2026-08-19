use std::sync::{Arc, RwLock};

use serde::Serialize;
use shortcut_windows::foreground::{foreground_process_is_elevated, process_is_elevated};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityStopReason {
    None,
    ProtectedInput,
    HigherIntegrityForeground,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub struct SecurityStatus {
    pub blocked: bool,
    pub reason: SecurityStopReason,
}

impl Default for SecurityStatus {
    fn default() -> Self {
        Self {
            blocked: false,
            reason: SecurityStopReason::None,
        }
    }
}

pub type SharedSecurityStatus = Arc<RwLock<SecurityStatus>>;

pub fn shared_security_status() -> SharedSecurityStatus {
    Arc::new(RwLock::new(SecurityStatus::default()))
}

pub struct SecurityGate {
    automation: Option<IUIAutomation>,
    app_is_elevated: bool,
    shared: SharedSecurityStatus,
}

impl SecurityGate {
    pub fn new(shared: SharedSecurityStatus) -> Self {
        // Worker thread owns this object. COM initialization failure intentionally leaves
        // automation unavailable; evaluation then fails closed rather than guessing safe.
        let automation = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
        };
        let app_is_elevated = process_is_elevated(std::process::id()).unwrap_or(false);
        Self {
            automation,
            app_is_elevated,
            shared,
        }
    }

    pub fn allows_input(&self) -> bool {
        let result = self.evaluate();
        if let Ok(mut status) = self.shared.write() {
            *status = result.clone();
        }
        !result.blocked
    }

    fn evaluate(&self) -> SecurityStatus {
        if !self.app_is_elevated {
            match foreground_process_is_elevated() {
                Ok(true) => {
                    return SecurityStatus {
                        blocked: true,
                        reason: SecurityStopReason::HigherIntegrityForeground,
                    }
                }
                Ok(false) => {}
                Err(_) => {
                    return SecurityStatus {
                        blocked: true,
                        reason: SecurityStopReason::Unknown,
                    }
                }
            }
        }

        let Some(automation) = self.automation.as_ref() else {
            return SecurityStatus {
                blocked: true,
                reason: SecurityStopReason::Unknown,
            };
        };
        let focused = unsafe { automation.GetFocusedElement() };
        let Ok(focused) = focused else {
            return SecurityStatus {
                blocked: true,
                reason: SecurityStopReason::Unknown,
            };
        };
        match unsafe { focused.CurrentIsPassword() } {
            Ok(value) if value.as_bool() => SecurityStatus {
                blocked: true,
                reason: SecurityStopReason::ProtectedInput,
            },
            Ok(_) => SecurityStatus::default(),
            Err(_) => SecurityStatus {
                blocked: true,
                reason: SecurityStopReason::Unknown,
            },
        }
    }
}
