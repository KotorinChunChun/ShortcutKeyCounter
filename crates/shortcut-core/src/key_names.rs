use crate::model::MainInput;

/// Windows Virtual-Key codeを、入力文字列を生成せずに安定した表示名へ変換する。
pub fn key_name(vk: u16) -> Option<String> {
    let name = match vk {
        0x30..=0x39 => return Some(char::from_u32(u32::from(vk)).unwrap().to_string()),
        0x41..=0x5A => return Some(char::from_u32(u32::from(vk)).unwrap().to_string()),
        0x70..=0x87 => return Some(format!("F{}", vk - 0x6F)),
        0x60..=0x69 => return Some(format!("Num{}", vk - 0x60)),
        0x09 => "Tab",
        0x03 => "Cancel",
        0x0C => "Clear",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x15 => "Kana",
        0x17 => "Junja",
        0x18 => "Final",
        0x19 => "Hanja",
        0x1A => "Kanji",
        0x1C => "Convert",
        0x1D => "NonConvert",
        0x1E => "Accept",
        0x1F => "ModeChange",
        0x1B => "Esc",
        0x20 => "Space",
        0x2A => "Print",
        0x2B => "Execute",
        0x2C => "PrintScreen",
        0x08 => "Backspace",
        0x2E => "Delete",
        0x2D => "Insert",
        0x2F => "Help",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x6A => "NumMultiply",
        0x6B => "NumAdd",
        0x6C => "NumSeparator",
        0x6D => "NumSubtract",
        0x6E => "NumDecimal",
        0x6F => "NumDivide",
        0x25 => "Left",
        0x27 => "Right",
        0x26 => "Up",
        0x28 => "Down",
        0x5D => "Apps",
        0x5F => "Sleep",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xA6 => "BrowserBack",
        0xA7 => "BrowserForward",
        0xA8 => "BrowserRefresh",
        0xA9 => "BrowserStop",
        0xAA => "BrowserSearch",
        0xAB => "BrowserFavorites",
        0xAC => "BrowserHome",
        0xAD => "VolumeMute",
        0xAE => "VolumeDown",
        0xAF => "VolumeUp",
        0xB0 => "MediaNext",
        0xB1 => "MediaPrevious",
        0xB2 => "MediaStop",
        0xB3 => "MediaPlayPause",
        0xB4 => "LaunchMail",
        0xB5 => "LaunchMedia",
        0xB6 => "LaunchApp1",
        0xB7 => "LaunchApp2",
        0xBA => "Semicolon",
        0xBB => "Equal",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Backquote",
        0xDB => "LeftBracket",
        0xDC => "Backslash",
        0xDD => "RightBracket",
        0xDE => "Quote",
        0xDF => "Oem8",
        0xE1 => "OemAx",
        0xE2 => "Oem102",
        0xE3 => "IcoHelp",
        0xE4 => "Ico00",
        0xE5 => "ProcessKey",
        0xE6 => "IcoClear",
        0xE7 => "Packet",
        0xE9 => "OemReset",
        0xEA => "OemJump",
        0xEB => "OemPa1",
        0xEC => "OemPa2",
        0xED => "OemPa3",
        0xEE => "OemWsCtrl",
        0xEF => "OemCuSel",
        0xF0 => "OemAttn",
        0xF1 => "OemFinish",
        0xF2 => "OemCopy",
        0xF3 => "OemAuto",
        0xF4 => "OemEnlw",
        0xF5 => "OemBackTab",
        _ => return None,
    };

    Some(name.to_owned())
}

/// 主入力を、Modifierを含まない安定した入力名へ変換する。
/// 名前を持たないVirtual-Keyは `Err` でそのcodeを返す。
pub fn main_input_name(input: &MainInput) -> Result<String, u16> {
    let name = match input {
        MainInput::Key(vk) => return key_name(*vk).ok_or(*vk),
        MainInput::MouseLeft => "MouseLeft",
        MainInput::MouseRight => "MouseRight",
        MainInput::MouseMiddle => "MouseMiddle",
        MainInput::MouseX1 => "MouseX1",
        MainInput::MouseX2 => "MouseX2",
        MainInput::WheelUp => "WheelUp",
        MainInput::WheelDown => "WheelDown",
        MainInput::WheelLeft => "WheelLeft",
        MainInput::WheelRight => "WheelRight",
    };
    Ok(name.to_owned())
}

/// 単独キー表示用の正規化。Virtual-Key値を文字入力へ変換せず、既存の安定したキー名を
/// そのまま返す。
pub fn normalize_single_key(vk: u32) -> Option<String> {
    u16::try_from(vk).ok().and_then(key_name)
}

/// MainInputとして扱わないModifierのVirtual-Key codeかどうかを判定する。
pub fn is_modifier_key(vk: u16) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
    )
}

/// キーマッピング画面とOverlay候補で共有する主入力カタログ。
pub fn key_catalog() -> Vec<(u16, String)> {
    let mut virtual_keys = Vec::new();
    virtual_keys.extend(0x30_u16..=0x39);
    virtual_keys.extend(0x41_u16..=0x5A);
    virtual_keys.extend(0x60_u16..=0x87);
    virtual_keys.extend(0x90_u16..=0x91);
    virtual_keys.extend(0xA6_u16..=0xB7);
    virtual_keys.extend(0xBA_u16..=0xC0);
    virtual_keys.extend(0xDB_u16..=0xF5);
    virtual_keys.extend([
        0x03, 0x09, 0x0C, 0x0D, 0x13, 0x14, 0x15, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E,
        0x2F, 0x5D, 0x5F,
    ]);
    virtual_keys.sort_unstable();
    virtual_keys.dedup();
    virtual_keys
        .into_iter()
        .filter_map(|virtual_key| key_name(virtual_key).map(|name| (virtual_key, name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_and_digits() {
        for vk in 0x41..=0x5A {
            assert_eq!(
                key_name(vk),
                Some(char::from_u32(u32::from(vk)).unwrap().to_string())
            );
        }
        for vk in 0x30..=0x39 {
            assert_eq!(
                key_name(vk),
                Some(char::from_u32(u32::from(vk)).unwrap().to_string())
            );
        }
    }

    #[test]
    fn maps_function_keys_at_both_ends() {
        assert_eq!(key_name(0x70), Some("F1".to_owned()));
        assert_eq!(key_name(0x87), Some("F24".to_owned()));
    }

    #[test]
    fn maps_special_keys_and_arrows() {
        let expected = [
            (0x09, "Tab"),
            (0x0D, "Enter"),
            (0x1B, "Esc"),
            (0x2E, "Delete"),
            (0x25, "Left"),
            (0x27, "Right"),
            (0x26, "Up"),
            (0x28, "Down"),
        ];

        for (vk, name) in expected {
            assert_eq!(key_name(vk), Some(name.to_owned()));
        }
    }

    #[test]
    fn modifier_keys_are_not_main_input_names() {
        assert!(is_modifier_key(0x11));
        assert!(is_modifier_key(0xA2));
        assert!(is_modifier_key(0x5B));
        assert!(!is_modifier_key(0x41));
        assert_eq!(key_name(0x11), None);
    }

    #[test]
    fn normalizes_single_key_without_text_input() {
        assert_eq!(normalize_single_key(0x41), Some("A".to_owned()));
        assert_eq!(normalize_single_key(0x0D), Some("Enter".to_owned()));
        assert_eq!(normalize_single_key(0x10000), None);
    }

    #[test]
    fn maps_oem_symbols_and_extended_keys() {
        assert_eq!(key_name(0xBA), Some("Semicolon".to_owned()));
        assert_eq!(key_name(0xBB), Some("Equal".to_owned()));
        assert_eq!(key_name(0xE2), Some("Oem102".to_owned()));
        assert!(key_catalog().iter().any(|(vk, _)| *vk == 0xBA));
        assert!(key_catalog().iter().any(|(vk, _)| *vk == 0xB3));
    }

    #[test]
    fn main_input_name_covers_keys_and_pointer_inputs() {
        assert_eq!(main_input_name(&MainInput::Key(0x41)), Ok("A".to_owned()));
        assert_eq!(main_input_name(&MainInput::Key(0x07)), Err(0x07));
        assert_eq!(main_input_name(&MainInput::Key(0x11)), Err(0x11));
        let pointers = [
            (MainInput::MouseLeft, "MouseLeft"),
            (MainInput::MouseRight, "MouseRight"),
            (MainInput::MouseMiddle, "MouseMiddle"),
            (MainInput::MouseX1, "MouseX1"),
            (MainInput::MouseX2, "MouseX2"),
            (MainInput::WheelUp, "WheelUp"),
            (MainInput::WheelDown, "WheelDown"),
            (MainInput::WheelLeft, "WheelLeft"),
            (MainInput::WheelRight, "WheelRight"),
        ];
        for (input, name) in pointers {
            assert_eq!(main_input_name(&input), Ok(name.to_owned()));
        }
    }
}
