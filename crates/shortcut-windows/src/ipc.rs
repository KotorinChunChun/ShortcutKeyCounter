//! Elevated capture helper用のNamed Pipe JSON Lines通信。

use std::ffi::c_void;
use std::sync::Arc;

use chrono::NaiveDateTime;
use shortcut_common::ipc_protocol::{
    validate_hello, validate_input, HelloMessage, InputMessage, MainInput as IpcMainInput,
    ProtocolMessage, ProtocolValidationError, MAX_IPC_MESSAGE_BYTES, PROTOCOL_VERSION,
};
use shortcut_core::model::{CapturedInput, MainInput, ModifierState, ScreenPoint};
use thiserror::Error;
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Com::CoCreateGuid;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_WAIT,
};
use windows::Win32::System::IO::CancelIoEx;

const PIPE_PREFIX: &str = r"\\.\pipe\ShortcutKeyCounter-capture-";

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("Named PipeのWindows APIに失敗しました: {0}")]
    Windows(#[source] windows::core::Error),
    #[error("Named Pipeハンドルを作成できません")]
    InvalidHandle,
    #[error("IPC JSONのシリアライズに失敗しました: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("IPC JSONのUTF-8復号に失敗しました")]
    InvalidUtf8,
    #[error("IPCメッセージのサイズ上限を超えました")]
    MessageTooLarge,
    #[error("IPCメッセージが空です")]
    EmptyMessage,
    #[error("IPCメッセージの種類が想定外です")]
    UnexpectedMessage,
    #[error("IPCプロトコル検証に失敗しました: {0:?}")]
    Validation(ProtocolValidationError),
    #[error("IPCの入力時刻を解釈できません")]
    InvalidTimestamp,
    #[error("Named Pipeへの書き込みが不完全です")]
    IncompleteWrite,
}

/// 1セッション分のNamed Pipe server。
pub struct CapturePipeServer {
    name: String,
    token: String,
    // `windows::Win32::Foundation::HANDLE` itself is !Send/!Sync because it wraps a raw
    // pointer. A kernel handle value is valid process-wide, so keep only its scalar value
    // and reconstruct HANDLE at each Win32 call. This allows the pipe reader to live on a
    // dedicated thread without an unsafe Send/Sync implementation.
    handle_value: isize,
}

impl CapturePipeServer {
    fn handle(&self) -> HANDLE {
        HANDLE(self.handle_value as *mut c_void)
    }
    /// ランダムGUIDを含むPipe名とtokenを生成してserverを作成する。
    pub fn new() -> Result<Self, IpcError> {
        let suffix = new_guid_string()?;
        let token = new_guid_string()?;
        let name = format!("{PIPE_PREFIX}{suffix}");
        let wide_name = wide_null(&name);
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide_name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1,
                MAX_IPC_MESSAGE_BYTES as u32,
                MAX_IPC_MESSAGE_BYTES as u32,
                0,
                None,
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.0.is_null() {
            return Err(IpcError::InvalidHandle);
        }
        Ok(Self {
            name,
            token,
            handle_value: handle.0 as isize,
        })
    }

    pub fn pipe_name(&self) -> &str {
        &self.name
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// helper接続を待ち、hello/tokenを検証した接続を返す。
    pub fn accept(self: &Arc<Self>) -> Result<AuthenticatedPipeConnection, IpcError> {
        if let Err(error) = unsafe { ConnectNamedPipe(self.handle(), None) } {
            // The elevated client may connect between CreateNamedPipeW and this call.
            // In that valid race ConnectNamedPipe reports ERROR_PIPE_CONNECTED.
            if error.code() != HRESULT::from_win32(ERROR_PIPE_CONNECTED.0) {
                return Err(IpcError::Windows(error));
            }
        }
        let connection = AuthenticatedPipeConnection {
            server: Arc::clone(self),
        };
        let hello = read_message(self.handle())?;
        let ProtocolMessage::Hello(hello) = hello else {
            return Err(IpcError::UnexpectedMessage);
        };
        validate_hello(&hello, &self.token).map_err(IpcError::Validation)?;
        write_message(
            self.handle(),
            &ProtocolMessage::HelloOk {
                protocol_version: PROTOCOL_VERSION,
            },
        )?;
        Ok(connection)
    }

    /// helperへ正常終了を要求する。切断前のbest-effort通知として使用する。
    pub fn request_shutdown(&self) -> Result<(), IpcError> {
        write_message(
            self.handle(),
            &ProtocolMessage::Shutdown {
                protocol_version: PROTOCOL_VERSION,
            },
        )
    }

    /// 別threadでblock中のReadFileを含む保留I/Oへcancelを要求する。
    /// Elevated OFF時のreader join待ちを解消するために使用する。
    pub fn cancel_pending_io(&self) {
        let _ = unsafe { CancelIoEx(self.handle(), None) };
    }

    /// 現在のセッションを切断する。reader停止時のblocking解除にも使用する。
    pub fn disconnect(&self) {
        let _ = unsafe { DisconnectNamedPipe(self.handle()) };
    }
}

impl Drop for CapturePipeServer {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle()) };
    }
}

/// hello検証済みserver側接続。
pub struct AuthenticatedPipeConnection {
    server: Arc<CapturePipeServer>,
}

impl AuthenticatedPipeConnection {
    /// helperから1入力を受信し、main processのCapturedInputへ変換する。
    pub fn read_input(&self) -> Result<CapturedInput, IpcError> {
        let message = read_message(self.server.handle())?;
        let ProtocolMessage::Input(input) = message else {
            return Err(IpcError::UnexpectedMessage);
        };
        input_message_to_captured_input(input)
    }
}

impl Drop for AuthenticatedPipeConnection {
    fn drop(&mut self) {
        let _ = unsafe { DisconnectNamedPipe(self.server.handle()) };
    }
}

/// helper側Named Pipe client。
pub struct CapturePipeClient {
    handle_value: isize,
}

impl CapturePipeClient {
    fn handle(&self) -> HANDLE {
        HANDLE(self.handle_value as *mut c_void)
    }
    pub fn connect(pipe_name: &str) -> Result<Self, IpcError> {
        let wide_name = wide_null(pipe_name);
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide_name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(IpcError::Windows)?;
        if handle == INVALID_HANDLE_VALUE || handle.0.is_null() {
            return Err(IpcError::InvalidHandle);
        }
        Ok(Self {
            handle_value: handle.0 as isize,
        })
    }

    /// helloを送り、serverからhello_okを受け取る。
    pub fn handshake(self, token: &str) -> Result<Self, IpcError> {
        write_message(
            self.handle(),
            &ProtocolMessage::Hello(HelloMessage {
                protocol_version: PROTOCOL_VERSION,
                token: token.to_owned(),
            }),
        )?;
        let response = read_message(self.handle())?;
        match response {
            ProtocolMessage::HelloOk { protocol_version }
                if protocol_version == PROTOCOL_VERSION =>
            {
                Ok(self)
            }
            _ => Err(IpcError::UnexpectedMessage),
        }
    }

    pub fn send_input(&self, input: InputMessage) -> Result<(), IpcError> {
        validate_input(&input).map_err(IpcError::Validation)?;
        write_message(self.handle(), &ProtocolMessage::Input(input))
    }

    /// main processからの正常終了要求を待つ。Pipe切断もhelper終了条件として上位へ返す。
    pub fn wait_for_shutdown(&self) -> Result<(), IpcError> {
        match read_message(self.handle())? {
            ProtocolMessage::Shutdown { protocol_version }
                if protocol_version == PROTOCOL_VERSION =>
            {
                Ok(())
            }
            _ => Err(IpcError::UnexpectedMessage),
        }
    }
}

impl Drop for CapturePipeClient {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle()) };
    }
}

/// 受信したIPC入力をmain processの内部型へ変換する。
pub fn input_message_to_captured_input(message: InputMessage) -> Result<CapturedInput, IpcError> {
    validate_input(&message).map_err(IpcError::Validation)?;
    let occurred_at = NaiveDateTime::parse_from_str(&message.occurred_at, "%Y-%m-%d %H:%M:%S%.3f")
        .map_err(|_| IpcError::InvalidTimestamp)?;
    let main_input = match message.main_input {
        IpcMainInput::Key { vk } => MainInput::Key(vk),
        IpcMainInput::MouseLeft => MainInput::MouseLeft,
        IpcMainInput::MouseRight => MainInput::MouseRight,
        IpcMainInput::MouseMiddle => MainInput::MouseMiddle,
        IpcMainInput::MouseX1 => MainInput::MouseX1,
        IpcMainInput::MouseX2 => MainInput::MouseX2,
        IpcMainInput::WheelUp => MainInput::WheelUp,
        IpcMainInput::WheelDown => MainInput::WheelDown,
        IpcMainInput::WheelLeft => MainInput::WheelLeft,
        IpcMainInput::WheelRight => MainInput::WheelRight,
    };
    Ok(CapturedInput {
        occurred_at,
        modifiers: ModifierState {
            ctrl: message.ctrl,
            alt: message.alt,
            win: message.win,
            shift: message.shift,
        },
        main_input,
        foreground_executable: message.application,
        pointer: match (message.pointer_x, message.pointer_y) {
            (Some(x), Some(y)) => Some(ScreenPoint { x, y }),
            _ => None,
        },
    })
}

fn read_message(handle: HANDLE) -> Result<ProtocolMessage, IpcError> {
    let mut buffer = vec![0_u8; MAX_IPC_MESSAGE_BYTES];
    let mut bytes_read = 0_u32;
    unsafe {
        ReadFile(
            handle,
            Some(buffer.as_mut_slice()),
            Some(&mut bytes_read),
            None,
        )
    }
    .map_err(IpcError::Windows)?;
    if bytes_read == 0 {
        return Err(IpcError::EmptyMessage);
    }
    buffer.truncate(bytes_read as usize);
    let line = String::from_utf8(buffer).map_err(|_| IpcError::InvalidUtf8)?;
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Err(IpcError::EmptyMessage);
    }
    serde_json::from_str(line).map_err(IpcError::Serialize)
}

fn write_message(handle: HANDLE, message: &ProtocolMessage) -> Result<(), IpcError> {
    let mut bytes = serde_json::to_vec(message)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_IPC_MESSAGE_BYTES {
        return Err(IpcError::MessageTooLarge);
    }
    let mut bytes_written = 0_u32;
    unsafe {
        WriteFile(
            handle,
            Some(bytes.as_slice()),
            Some(&mut bytes_written),
            None,
        )
    }
    .map_err(IpcError::Windows)?;
    if bytes_written as usize != bytes.len() {
        return Err(IpcError::IncompleteWrite);
    }
    unsafe { FlushFileBuffers(handle) }.map_err(IpcError::Windows)
}

fn new_guid_string() -> Result<String, IpcError> {
    Ok(format!(
        "{:032x}",
        unsafe { CoCreateGuid() }
            .map_err(IpcError::Windows)?
            .to_u128()
    ))
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shortcut_common::ipc_protocol::InputMessage;

    fn input_message() -> InputMessage {
        InputMessage {
            protocol_version: PROTOCOL_VERSION,
            occurred_at: "2026-08-15 09:14:21.152".to_owned(),
            ctrl: true,
            alt: false,
            win: false,
            shift: false,
            main_input: IpcMainInput::Key { vk: 67 },
            application: "Code.exe".to_owned(),
            pointer_x: None,
            pointer_y: None,
        }
    }

    #[test]
    fn converts_valid_input_without_receiving_shortcut_string() {
        let input = input_message_to_captured_input(input_message()).expect("input");
        assert_eq!(
            input
                .occurred_at
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string(),
            "2026-08-15 09:14:21.152"
        );
        assert_eq!(input.main_input, MainInput::Key(67));
        assert_eq!(input.foreground_executable, "Code.exe");
    }

    #[test]
    fn conversion_rejects_invalid_application_and_timestamp() {
        let mut input = input_message();
        input.application = r"C:\Windows\Code.exe".to_owned();
        assert!(matches!(
            input_message_to_captured_input(input),
            Err(IpcError::Validation(
                ProtocolValidationError::ApplicationContainsPathSeparator
            ))
        ));

        let mut input = input_message();
        input.occurred_at = "not-a-timestamp".to_owned();
        assert!(matches!(
            input_message_to_captured_input(input),
            Err(IpcError::InvalidTimestamp)
        ));
    }

    #[test]
    fn server_name_and_token_are_session_specific() {
        let first = CapturePipeServer::new().expect("first server");
        let second = CapturePipeServer::new().expect("second server");
        assert!(first.pipe_name().starts_with(PIPE_PREFIX));
        assert_ne!(first.pipe_name(), second.pipe_name());
        assert_ne!(first.token(), second.token());
    }

    #[test]
    #[ignore = "Named Pipeのserver/client接続をWindows手動スモーク環境で確認する"]
    fn server_and_client_can_handshake_and_exchange_input() {
        let server = Arc::new(CapturePipeServer::new().expect("server"));
        let pipe_name = server.pipe_name().to_owned();
        let token = server.token().to_owned();
        let client_thread = std::thread::spawn(move || {
            let client = CapturePipeClient::connect(&pipe_name)
                .expect("connect")
                .handshake(&token)
                .expect("handshake");
            client.send_input(input_message()).expect("send input");
        });
        let connection = server.accept().expect("accept");
        let input = connection.read_input().expect("receive input");
        assert_eq!(input.main_input, MainInput::Key(67));
        client_thread.join().expect("client thread");
    }
}
