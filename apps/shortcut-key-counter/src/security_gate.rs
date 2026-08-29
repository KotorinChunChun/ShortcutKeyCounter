use std::sync::{Arc, RwLock};

use serde::Serialize;
use shortcut_windows::foreground::{foreground_process_is_elevated, process_is_elevated};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

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

/// UI Automationで取得した保護入力の検査結果。
///
/// `foreground_window_contains_password` はForeground WindowのRoot要素自身と、
/// そのDescendantsにpassword属性の要素が存在することを表す。focused要素は
/// Foreground Windowのツリーから外れていても独立して判定し、呼び出し側でORする。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PasswordInspection {
    foreground_window_contains_password: bool,
    focused_is_password: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PasswordInspectionError;

/// SecurityGateからUI Automation実装を分離するための境界。
///
/// UIAのCOMオブジェクトを判定ロジックへ持ち込まず、テストではfakeを注入できる。
trait PasswordInspector {
    fn inspect(&self) -> Result<PasswordInspection, PasswordInspectionError>;
}

/// Windows UI Automationを担当する小さな実装。
///
/// UIAの取得・列挙で一つでも失敗した場合は結果を返さず、SecurityGate側の
/// Fail Closed（Unknown）へ委ねる。ウィンドウタイトルや入力値は取得・保持しない。
struct WindowsPasswordInspector {
    automation: IUIAutomation,
}

impl WindowsPasswordInspector {
    fn new() -> Option<Self> {
        // Capture worker threadはUI threadとは別なので、このthreadでCOMを初期化する。
        // 初期化できない場合はUIAを安全に利用できないためNone（Fail Closed）とする。
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if initialized.is_err() {
            return None;
        }
        let automation = unsafe {
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        }
        .ok()?;
        Some(Self { automation })
    }

    fn inspect_window_contains_password(
        &self,
        root: &IUIAutomationElement,
    ) -> Result<bool, PasswordInspectionError> {
        let root_is_password = unsafe { root.CurrentIsPassword() }
            .map_err(|_| PasswordInspectionError)?
            .as_bool();
        if root_is_password {
            return Ok(true);
        }

        let condition = unsafe { self.automation.CreateTrueCondition() }
            .map_err(|_| PasswordInspectionError)?;
        let descendants = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|_| PasswordInspectionError)?;
        let length = unsafe { descendants.Length() }.map_err(|_| PasswordInspectionError)?;
        for index in 0..length {
            let element =
                unsafe { descendants.GetElement(index) }.map_err(|_| PasswordInspectionError)?;
            let is_password = unsafe { element.CurrentIsPassword() }
                .map_err(|_| PasswordInspectionError)?
                .as_bool();
            if is_password {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl PasswordInspector for WindowsPasswordInspector {
    fn inspect(&self) -> Result<PasswordInspection, PasswordInspectionError> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground == HWND::default() {
            return Err(PasswordInspectionError);
        }

        let root = unsafe { self.automation.ElementFromHandle(foreground) }
            .map_err(|_| PasswordInspectionError)?;
        let foreground_window_contains_password = self.inspect_window_contains_password(&root)?;

        // focused elementの判定はForeground Window配下の列挙とは独立させる。
        // これにより、UIAのツリー境界やブラウザ実装差があっても既存の
        // focused password保護を失わない。
        let focused =
            unsafe { self.automation.GetFocusedElement() }.map_err(|_| PasswordInspectionError)?;
        let focused_is_password = unsafe { focused.CurrentIsPassword() }
            .map_err(|_| PasswordInspectionError)?
            .as_bool();

        Ok(PasswordInspection {
            foreground_window_contains_password,
            focused_is_password,
        })
    }
}

pub struct SecurityGate {
    password_inspector: Option<Box<dyn PasswordInspector>>,
    app_is_elevated: Result<bool, ()>,
    shared: SharedSecurityStatus,
}

impl SecurityGate {
    pub fn new(shared: SharedSecurityStatus) -> Self {
        Self {
            password_inspector: WindowsPasswordInspector::new()
                .map(|inspector| Box::new(inspector) as Box<dyn PasswordInspector>),
            app_is_elevated: process_is_elevated(std::process::id()).map_err(|_| ()),
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
        let app_is_elevated = match self.app_is_elevated {
            Ok(value) => value,
            Err(_) => return unknown_status(),
        };
        if !app_is_elevated {
            match foreground_process_is_elevated() {
                Ok(true) => {
                    return SecurityStatus {
                        blocked: true,
                        reason: SecurityStopReason::HigherIntegrityForeground,
                    }
                }
                Ok(false) => {}
                Err(_) => return unknown_status(),
            }
        }

        let Some(inspector) = self.password_inspector.as_ref() else {
            return unknown_status();
        };
        match inspector.inspect() {
            Ok(inspection)
                if inspection.foreground_window_contains_password
                    || inspection.focused_is_password =>
            {
                SecurityStatus {
                    blocked: true,
                    reason: SecurityStopReason::ProtectedInput,
                }
            }
            Ok(_) => SecurityStatus::default(),
            Err(_) => unknown_status(),
        }
    }

    #[cfg(test)]
    fn with_inspector(
        shared: SharedSecurityStatus,
        app_is_elevated: Result<bool, ()>,
        inspector: Box<dyn PasswordInspector>,
    ) -> Self {
        Self {
            password_inspector: Some(inspector),
            app_is_elevated,
            shared,
        }
    }
}

fn unknown_status() -> SecurityStatus {
    SecurityStatus {
        blocked: true,
        reason: SecurityStopReason::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakePasswordInspector {
        result: Result<PasswordInspection, PasswordInspectionError>,
    }

    impl PasswordInspector for FakePasswordInspector {
        fn inspect(&self) -> Result<PasswordInspection, PasswordInspectionError> {
            self.result
        }
    }

    fn gate(result: Result<PasswordInspection, PasswordInspectionError>) -> SecurityGate {
        SecurityGate::with_inspector(
            shared_security_status(),
            Ok(true),
            Box::new(FakePasswordInspector { result }),
        )
    }

    #[test]
    fn foreground_window_password_blocks_even_when_focus_is_elsewhere() {
        let gate = gate(Ok(PasswordInspection {
            foreground_window_contains_password: true,
            focused_is_password: false,
        }));

        assert!(!gate.allows_input());
    }

    #[test]
    fn focused_password_remains_an_independent_blocking_signal() {
        let gate = gate(Ok(PasswordInspection {
            foreground_window_contains_password: false,
            focused_is_password: true,
        }));

        assert!(!gate.allows_input());
    }

    #[test]
    fn no_password_allows_input_when_all_inspection_is_known() {
        let gate = gate(Ok(PasswordInspection {
            foreground_window_contains_password: false,
            focused_is_password: false,
        }));

        assert!(gate.allows_input());
    }

    #[test]
    fn failed_inspection_fails_closed() {
        let gate = gate(Err(PasswordInspectionError));

        assert!(!gate.allows_input());
    }

    #[test]
    fn failed_app_integrity_check_fails_closed_before_inspection() {
        let gate = SecurityGate::with_inspector(
            shared_security_status(),
            Err(()),
            Box::new(FakePasswordInspector {
                result: Ok(PasswordInspection {
                    foreground_window_contains_password: false,
                    focused_is_password: false,
                }),
            }),
        );

        assert!(!gate.allows_input());
    }
}
