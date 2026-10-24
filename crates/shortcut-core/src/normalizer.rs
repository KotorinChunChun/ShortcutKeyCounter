use std::fmt;

use crate::eligibility::is_recordable;
use crate::key_names::{is_modifier_key, main_input_name};
use crate::model::{MainInput, ModifierState};

/// 正規化できない入力の理由。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizeError {
    IneligibleInput,
    UnsupportedKey(u16),
}

impl fmt::Display for NormalizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IneligibleInput => formatter.write_str("入力は記録対象外です"),
            Self::UnsupportedKey(vk) => write!(formatter, "未対応のVirtual-Key codeです: {vk}"),
        }
    }
}

impl std::error::Error for NormalizeError {}

/// Modifierと主入力を仕様で定めた順序のショートカット文字列へ変換する。
pub fn normalize(
    modifiers: &ModifierState,
    main_input: &MainInput,
) -> Result<String, NormalizeError> {
    if !is_recordable(modifiers, main_input) {
        return Err(NormalizeError::IneligibleInput);
    }

    normalize_input(modifiers, main_input)
}

/// Modifierの有無にかかわらず、キーマッピング／Overlay用の安定した入力名へ変換する。
pub fn normalize_input(
    modifiers: &ModifierState,
    main_input: &MainInput,
) -> Result<String, NormalizeError> {
    if matches!(main_input, MainInput::Key(vk) if is_modifier_key(*vk)) {
        return Err(NormalizeError::IneligibleInput);
    }

    let mut parts = Vec::with_capacity(5);
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.win {
        parts.push("Win");
    }
    if modifiers.shift {
        parts.push("Shift");
    }

    let main_name = main_input_name(main_input).map_err(NormalizeError::UnsupportedKey)?;
    parts.push(main_name.as_str());

    Ok(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_order_is_independent_of_input_order() {
        let modifiers = ModifierState {
            ctrl: true,
            alt: true,
            win: false,
            shift: false,
        };

        assert_eq!(
            normalize(&modifiers, &MainInput::Key(0x53)),
            Ok("Ctrl+Alt+S".to_owned())
        );
    }

    #[test]
    fn normalizes_keyboard_examples() {
        let cases = [
            (
                ModifierState {
                    ctrl: true,
                    alt: false,
                    win: false,
                    shift: true,
                },
                MainInput::Key(0x43),
                "Ctrl+Shift+C",
            ),
            (
                ModifierState {
                    ctrl: false,
                    alt: false,
                    win: true,
                    shift: true,
                },
                MainInput::Key(0x53),
                "Win+Shift+S",
            ),
            (
                ModifierState {
                    ctrl: true,
                    alt: false,
                    win: true,
                    shift: false,
                },
                MainInput::Key(0x44),
                "Ctrl+Win+D",
            ),
            (
                ModifierState {
                    ctrl: true,
                    alt: true,
                    win: false,
                    shift: true,
                },
                MainInput::Key(0x54),
                "Ctrl+Alt+Shift+T",
            ),
        ];

        for (modifiers, input, expected) in cases {
            assert_eq!(normalize(&modifiers, &input), Ok(expected.to_owned()));
        }
    }

    #[test]
    fn normalizes_mouse_input() {
        let modifiers = ModifierState {
            ctrl: true,
            alt: false,
            win: false,
            shift: false,
        };

        assert_eq!(
            normalize(&modifiers, &MainInput::MouseLeft),
            Ok("Ctrl+MouseLeft".to_owned())
        );
        assert_eq!(
            normalize(&modifiers, &MainInput::MouseMiddle),
            Ok("Ctrl+MouseMiddle".to_owned())
        );
    }

    #[test]
    fn normalizes_unmodified_input_for_overlay_policy() {
        assert_eq!(
            normalize_input(&ModifierState::default(), &MainInput::Key(0x41)),
            Ok("A".to_owned())
        );
        assert_eq!(
            normalize_input(
                &ModifierState {
                    shift: true,
                    ..ModifierState::default()
                },
                &MainInput::Key(0x41),
            ),
            Ok("Shift+A".to_owned())
        );
    }

    #[test]
    fn rejects_inputs_that_cannot_be_recorded() {
        let no_modifier = ModifierState::default();
        assert_eq!(
            normalize(&no_modifier, &MainInput::Key(0x41)),
            Err(NormalizeError::IneligibleInput)
        );

        let ctrl = ModifierState {
            ctrl: true,
            ..ModifierState::default()
        };
        assert_eq!(
            normalize(&ctrl, &MainInput::Key(0x11)),
            Err(NormalizeError::IneligibleInput)
        );
    }
}
