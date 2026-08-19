//! プロセス間通信で使用する JSON Lines プロトコル。

use serde::{Deserialize, Serialize};

/// IPC プロトコルのバージョン。
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_IPC_MESSAGE_BYTES: usize = 64 * 1024;

/// helloメッセージ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMessage {
    pub protocol_version: u8,
    pub token: String,
}

/// 入力されたメイン入力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MainInput {
    Key { vk: u16 },
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

/// 入力メッセージ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputMessage {
    pub protocol_version: u8,
    pub occurred_at: String,
    pub ctrl: bool,
    pub alt: bool,
    pub win: bool,
    pub shift: bool,
    pub main_input: MainInput,
    pub application: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_y: Option<i32>,
}

/// IPC で送受信する1行のメッセージ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    Hello(HelloMessage),
    HelloOk { protocol_version: u8 },
    Input(InputMessage),
    Shutdown { protocol_version: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolValidationError {
    UnsupportedVersion,
    EmptyToken,
    TokenMismatch,
    EmptyOccurredAt,
    EmptyApplication,
    ApplicationContainsPathSeparator,
    ApplicationContainsNul,
    ApplicationTooLong,
    InvalidVirtualKey,
}

pub fn validate_hello(
    message: &HelloMessage,
    expected_token: &str,
) -> Result<(), ProtocolValidationError> {
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolValidationError::UnsupportedVersion);
    }
    if message.token.is_empty() {
        return Err(ProtocolValidationError::EmptyToken);
    }
    if message.token != expected_token {
        return Err(ProtocolValidationError::TokenMismatch);
    }
    Ok(())
}

pub fn validate_input(message: &InputMessage) -> Result<(), ProtocolValidationError> {
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolValidationError::UnsupportedVersion);
    }
    if message.occurred_at.is_empty() {
        return Err(ProtocolValidationError::EmptyOccurredAt);
    }
    if message.application.trim().is_empty() {
        return Err(ProtocolValidationError::EmptyApplication);
    }
    if message.application.len() > 260 {
        return Err(ProtocolValidationError::ApplicationTooLong);
    }
    if message
        .application
        .chars()
        .any(|character| matches!(character, '\\' | '/' | '\0'))
    {
        return if message.application.contains('\0') {
            Err(ProtocolValidationError::ApplicationContainsNul)
        } else {
            Err(ProtocolValidationError::ApplicationContainsPathSeparator)
        };
    }
    if matches!(message.main_input, MainInput::Key { vk: 0 }) {
        return Err(ProtocolValidationError::InvalidVirtualKey);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_json_roundtrip() {
        let message = ProtocolMessage::Hello(HelloMessage {
            protocol_version: PROTOCOL_VERSION,
            token: "test-token".to_owned(),
        });
        let json = serde_json::to_string(&message).unwrap();

        assert_eq!(
            json,
            r#"{"type":"hello","protocol_version":1,"token":"test-token"}"#
        );
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn hello_ok_json_roundtrip() {
        let message = ProtocolMessage::HelloOk {
            protocol_version: PROTOCOL_VERSION,
        };
        let json = serde_json::to_string(&message).unwrap();

        assert_eq!(json, r#"{"type":"hello_ok","protocol_version":1}"#);
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn shutdown_json_roundtrip() {
        let message = ProtocolMessage::Shutdown {
            protocol_version: PROTOCOL_VERSION,
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(json, r#"{"type":"shutdown","protocol_version":1}"#);
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn input_json_roundtrip() {
        let message = ProtocolMessage::Input(InputMessage {
            protocol_version: PROTOCOL_VERSION,
            occurred_at: "2026-08-15 09:14:21.152".to_owned(),
            ctrl: true,
            alt: false,
            win: false,
            shift: false,
            main_input: MainInput::Key { vk: 67 },
            application: "EXCEL.EXE".to_owned(),
            pointer_x: None,
            pointer_y: None,
        });
        let json = serde_json::to_string(&message).unwrap();

        assert_eq!(
            json,
            r#"{"type":"input","protocol_version":1,"occurred_at":"2026-08-15 09:14:21.152","ctrl":true,"alt":false,"win":false,"shift":false,"main_input":{"kind":"key","vk":67},"application":"EXCEL.EXE"}"#
        );
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn hello_validation_rejects_wrong_token_and_version() {
        let message = HelloMessage {
            protocol_version: PROTOCOL_VERSION,
            token: "wrong".to_owned(),
        };
        assert_eq!(
            validate_hello(&message, "expected"),
            Err(ProtocolValidationError::TokenMismatch)
        );
        let message = HelloMessage {
            protocol_version: PROTOCOL_VERSION + 1,
            token: "expected".to_owned(),
        };
        assert_eq!(
            validate_hello(&message, "expected"),
            Err(ProtocolValidationError::UnsupportedVersion)
        );
    }

    #[test]
    fn input_validation_rejects_path_and_empty_application() {
        let mut message = InputMessage {
            protocol_version: PROTOCOL_VERSION,
            occurred_at: "2026-08-15 09:14:21.152".to_owned(),
            ctrl: true,
            alt: false,
            win: false,
            shift: false,
            main_input: MainInput::Key { vk: 67 },
            application: r"C:\Windows\notepad.exe".to_owned(),
            pointer_x: None,
            pointer_y: None,
        };
        assert_eq!(
            validate_input(&message),
            Err(ProtocolValidationError::ApplicationContainsPathSeparator)
        );
        message.application = "   ".to_owned();
        assert_eq!(
            validate_input(&message),
            Err(ProtocolValidationError::EmptyApplication)
        );
    }

    #[test]
    fn input_validation_rejects_zero_virtual_key() {
        let message = InputMessage {
            protocol_version: PROTOCOL_VERSION,
            occurred_at: "2026-08-15 09:14:21.152".to_owned(),
            ctrl: true,
            alt: false,
            win: false,
            shift: false,
            main_input: MainInput::Key { vk: 0 },
            application: "notepad.exe".to_owned(),
            pointer_x: None,
            pointer_y: None,
        };
        assert_eq!(
            validate_input(&message),
            Err(ProtocolValidationError::InvalidVirtualKey)
        );
    }

    #[test]
    fn middle_click_json_roundtrip() {
        let message = ProtocolMessage::Input(InputMessage {
            protocol_version: PROTOCOL_VERSION,
            occurred_at: "2026-08-15 09:14:21.152".to_owned(),
            ctrl: true,
            alt: false,
            win: false,
            shift: true,
            main_input: MainInput::MouseMiddle,
            application: "Code.exe".to_owned(),
            pointer_x: Some(10),
            pointer_y: Some(20),
        });
        let json = serde_json::to_string(&message).expect("serialize");
        assert_eq!(
            serde_json::from_str::<ProtocolMessage>(&json).unwrap(),
            message
        );
    }
}
