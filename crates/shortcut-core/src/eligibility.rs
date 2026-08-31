use crate::model::{MainInput, ModifierState};

// Windows Virtual-Key codes for modifier keys.  Keeping these local avoids
// coupling the core crate to a Windows-only dependency.
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_LSHIFT: u16 = 0xA0;
const VK_RSHIFT: u16 = 0xA1;
const VK_LCONTROL: u16 = 0xA2;
const VK_RCONTROL: u16 = 0xA3;
const VK_LMENU: u16 = 0xA4;
const VK_RMENU: u16 = 0xA5;

fn is_modifier_key(vk: u16) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_CONTROL
            | VK_MENU
            | VK_LWIN
            | VK_RWIN
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
    )
}

/// 修飾なしでは永続記録しない文字入力系のVirtual-Key。
/// 英字・数字・OEM記号・テンキーの数字／演算子を対象とする。
pub fn is_unmodified_recording_prohibited(vk: u16) -> bool {
    matches!(
        vk,
        0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF | 0xE1..=0xE2
    )
}

/// 正規化済みidentityが、修飾なしでは永続記録しない英数記号か判定する。
pub fn is_unmodified_recording_prohibited_identity(identity: &str) -> bool {
    if identity.contains('+') {
        return false;
    }
    crate::key_names::key_catalog()
        .into_iter()
        .any(|(vk, name)| name == identity && is_unmodified_recording_prohibited(vk))
}

/// 正規化済みidentityが、修飾なしで記録可能な既知の主入力か判定する。
pub fn is_unmodified_recordable_identity(identity: &str) -> bool {
    if matches!(
        identity,
        "MouseLeft"
            | "MouseRight"
            | "MouseMiddle"
            | "MouseX1"
            | "MouseX2"
            | "WheelUp"
            | "WheelDown"
            | "WheelLeft"
            | "WheelRight"
    ) {
        return true;
    }
    crate::key_names::key_catalog()
        .into_iter()
        .any(|(vk, name)| name == identity && !is_unmodified_recording_prohibited(vk))
}

/// 入力が記録対象となるショートカットか判定する。
pub fn is_recordable(modifiers: &ModifierState, main_input: &MainInput) -> bool {
    match main_input {
        MainInput::Key(vk) => {
            if is_modifier_key(*vk) {
                return false;
            }
            modifiers.ctrl
                || modifiers.alt
                || modifiers.win
                || modifiers.shift
                || !is_unmodified_recording_prohibited(*vk)
        }
        MainInput::MouseLeft
        | MainInput::MouseRight
        | MainInput::MouseMiddle
        | MainInput::MouseX1
        | MainInput::MouseX2
        | MainInput::WheelUp
        | MainInput::WheelDown
        | MainInput::WheelLeft
        | MainInput::WheelRight => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifiers(ctrl: bool, alt: bool, win: bool, shift: bool) -> ModifierState {
        ModifierState {
            ctrl,
            alt,
            win,
            shift,
        }
    }

    #[test]
    fn ctrl_c_is_recordable() {
        assert!(is_recordable(
            &modifiers(true, false, false, false),
            &MainInput::Key(0x43)
        ));
    }

    #[test]
    fn ctrl_shift_c_is_recordable() {
        assert!(is_recordable(
            &modifiers(true, false, false, true),
            &MainInput::Key(0x43)
        ));
    }

    #[test]
    fn alt_tab_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, true, false, false),
            &MainInput::Key(0x09)
        ));
    }

    #[test]
    fn win_e_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, true, false),
            &MainInput::Key(0x45)
        ));
    }

    #[test]
    fn ctrl_mouse_left_is_recordable() {
        assert!(is_recordable(
            &modifiers(true, false, false, false),
            &MainInput::MouseLeft,
        ));
    }

    #[test]
    fn alt_wheel_down_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, true, false, false),
            &MainInput::WheelDown,
        ));
    }

    #[test]
    fn bare_a_is_not_recordable() {
        assert!(!is_recordable(
            &modifiers(false, false, false, false),
            &MainInput::Key(0x41)
        ));
    }

    #[test]
    fn bare_f5_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, false, false),
            &MainInput::Key(0x74)
        ));
    }

    #[test]
    fn shift_a_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, false, true),
            &MainInput::Key(0x41)
        ));
    }

    #[test]
    fn bare_mouse_left_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, false, false),
            &MainInput::MouseLeft,
        ));
    }

    #[test]
    fn shift_mouse_left_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, false, true),
            &MainInput::MouseLeft,
        ));
    }

    #[test]
    fn bare_wheel_is_recordable() {
        assert!(is_recordable(
            &modifiers(false, false, false, false),
            &MainInput::WheelLeft,
        ));
    }

    #[test]
    fn bare_letters_digits_symbols_and_numpad_are_not_recordable() {
        for vk in [0x41, 0x30, 0xBA, 0x60, 0x6B, 0xE1, 0xE2] {
            assert!(!is_recordable(
                &modifiers(false, false, false, false),
                &MainInput::Key(vk)
            ));
        }
    }

    #[test]
    fn normalized_identity_uses_the_same_unmodified_boundary() {
        for identity in ["A", "0", "Semicolon", "OemAx", "Oem102", "Num1", "NumAdd"] {
            assert!(
                is_unmodified_recording_prohibited_identity(identity),
                "{identity}"
            );
            assert!(!is_unmodified_recordable_identity(identity), "{identity}");
        }
        for identity in ["F5", "Enter", "MouseLeft", "WheelLeft"] {
            assert!(
                !is_unmodified_recording_prohibited_identity(identity),
                "{identity}"
            );
            assert!(is_unmodified_recordable_identity(identity), "{identity}");
        }
        assert!(!is_unmodified_recordable_identity("UnknownInput"));
    }
}
