//! キーマッピング画面専用の物理キー検索Capture。
//!
//! armed中の入力はSecurity Gate通過後にworkerで完全消費し、
//! Overlay / DB / Realtime Log / 統計へ流さない。

use std::sync::{Arc, Mutex};

use shortcut_core::key_names::normalize_single_key;
use shortcut_core::model::{CapturedInput, MainInput};

#[derive(Default)]
struct KeySearchInner {
    armed: bool,
    result: Option<String>,
}

#[derive(Clone, Default)]
pub struct KeySearchCapture {
    inner: Arc<Mutex<KeySearchInner>>,
}

impl KeySearchCapture {
    pub fn begin(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.armed = true;
            inner.result = None;
        }
    }

    pub fn cancel(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.armed = false;
            inner.result = None;
        }
    }

    pub fn take_result(&self) -> Option<String> {
        self.inner.lock().ok()?.result.take()
    }

    /// armed中なら必ずtrueを返して入力を通常経路から遮断する。
    /// modifier-only等、一覧の主入力identityへ変換できない入力は結果にせず待機を継続する。
    pub fn consume_if_armed(&self, input: &CapturedInput) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        if !inner.armed {
            return false;
        }
        if let Some(identity) = main_input_identity(&input.main_input) {
            inner.result = Some(identity);
            inner.armed = false;
        }
        true
    }
}

fn main_input_identity(input: &MainInput) -> Option<String> {
    Some(match input {
        MainInput::Key(vk) => normalize_single_key(u32::from(*vk))?,
        MainInput::MouseLeft => "MouseLeft".to_owned(),
        MainInput::MouseRight => "MouseRight".to_owned(),
        MainInput::MouseMiddle => "MouseMiddle".to_owned(),
        MainInput::MouseX1 => "MouseX1".to_owned(),
        MainInput::MouseX2 => "MouseX2".to_owned(),
        MainInput::WheelUp => "WheelUp".to_owned(),
        MainInput::WheelDown => "WheelDown".to_owned(),
        MainInput::WheelLeft => "WheelLeft".to_owned(),
        MainInput::WheelRight => "WheelRight".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use shortcut_core::model::ModifierState;

    fn input(main_input: MainInput) -> CapturedInput {
        CapturedInput {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 18)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            modifiers: ModifierState::default(),
            main_input,
            foreground_executable: "Code.exe".to_owned(),
            pointer: None,
        }
    }

    #[test]
    fn armed_search_consumes_and_returns_main_identity() {
        let search = KeySearchCapture::default();
        search.begin();
        assert!(search.consume_if_armed(&input(MainInput::Key(0x43))));
        assert_eq!(search.take_result().as_deref(), Some("C"));
        assert!(!search.consume_if_armed(&input(MainInput::Key(0x44))));
    }

    #[test]
    fn mouse_identity_is_supported() {
        let search = KeySearchCapture::default();
        search.begin();
        assert!(search.consume_if_armed(&input(MainInput::MouseLeft)));
        assert_eq!(search.take_result().as_deref(), Some("MouseLeft"));
    }
}
