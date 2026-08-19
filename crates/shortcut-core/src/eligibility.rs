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

/// 入力が記録対象となるショートカットか判定する。
pub fn is_recordable(modifiers: &ModifierState, main_input: &MainInput) -> bool {
    if !(modifiers.ctrl || modifiers.alt || modifiers.win || modifiers.shift) {
        return false;
    }

    match main_input {
        MainInput::Key(vk) => !is_modifier_key(*vk),
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
    fn bare_f5_is_not_recordable() {
        assert!(!is_recordable(
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
    fn bare_mouse_left_is_not_recordable() {
        assert!(!is_recordable(
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
}
