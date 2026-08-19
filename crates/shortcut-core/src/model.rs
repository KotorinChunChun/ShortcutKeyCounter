use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 左右の差を吸収したModifier状態。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModifierState {
    pub ctrl: bool,
    pub alt: bool,
    pub win: bool,
    pub shift: bool,
}

/// ショートカットの主入力。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MainInput {
    Key(u16),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseX1,
    MouseX2,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// Hookからworkerへ渡す入力情報。
///
/// 通常の入力文字列、ウィンドウタイトル、URLなどは保持しない。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapturedInput {
    pub occurred_at: NaiveDateTime,
    pub modifiers: ModifierState,
    pub main_input: MainInput,
    pub foreground_executable: String,
    #[serde(default)]
    pub pointer: Option<ScreenPoint>,
}

/// DB writerへ渡す、保存を許可されたイベント情報。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordableEvent {
    pub occurred_at: NaiveDateTime,
    pub shortcut: String,
    pub application: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn model_structs_can_be_created() {
        let modifiers = ModifierState {
            ctrl: true,
            alt: false,
            win: false,
            shift: true,
        };
        let input = CapturedInput {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("有効な日付")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("有効な時刻"),
            modifiers: modifiers.clone(),
            main_input: MainInput::Key(67),
            foreground_executable: "EXCEL.EXE".to_owned(),
            pointer: None,
        };
        let event = RecordableEvent {
            occurred_at: input.occurred_at,
            shortcut: "Ctrl+Shift+C".to_owned(),
            application: input.foreground_executable.clone(),
        };

        assert_eq!(input.modifiers, modifiers);
        assert_eq!(event.application, "EXCEL.EXE");
    }

    #[test]
    fn captured_input_serde_roundtrip_preserves_data() {
        let original = CapturedInput {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("有効な日付")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("有効な時刻"),
            modifiers: ModifierState {
                ctrl: true,
                alt: false,
                win: false,
                shift: false,
            },
            main_input: MainInput::MouseLeft,
            foreground_executable: "Code.exe".to_owned(),
            pointer: None,
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: CapturedInput = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored, original);
    }

    #[test]
    fn recordable_event_serde_roundtrip_preserves_data() {
        let original = RecordableEvent {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("有効な日付")
                .and_hms_milli_opt(9, 20, 44, 211)
                .expect("有効な時刻"),
            shortcut: "Ctrl+MouseLeft".to_owned(),
            application: "msedge.exe".to_owned(),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: RecordableEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored, original);
    }
}
