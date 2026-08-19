use std::env;
use std::sync::Arc;
use std::thread;

use chrono::NaiveDateTime;
use crossbeam_channel::{bounded, select, Receiver};
use shortcut_common::ipc_protocol::{InputMessage, MainInput as IpcMainInput, PROTOCOL_VERSION};
use shortcut_core::model::{CapturedInput, MainInput};
use shortcut_windows::ipc::CapturePipeClient;
use shortcut_windows::keyboard_hook::{
    start_keyboard_hook, stop_keyboard_hook, KeyboardHookHandle,
};
use shortcut_windows::mouse_hook::{start_mouse_hook, stop_mouse_hook, MouseHookHandle};

const HELPER_QUEUE_CAPACITY: usize = 1024;

#[derive(Debug, Eq, PartialEq)]
struct HelperArguments {
    pipe_name: String,
    token: String,
}

fn print_help() {
    println!("ShortcutKeyCapture - 入力取得ヘルパー");
    println!();
    println!("使い方: shortcut-key-capture --pipe <pipe-name> --token <token>");
    println!("        shortcut-key-capture --help");
}

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();

    if arguments.as_slice() == ["--help"] || arguments.as_slice() == ["-h"] {
        print_help();
        return;
    }

    let parsed = match parse_arguments(&arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = run_helper(parsed) {
        eprintln!("ShortcutKeyCaptureを開始できません: {error}");
        std::process::exit(1);
    }
}

fn parse_arguments(arguments: &[String]) -> Result<HelperArguments, String> {
    if arguments.len() != 4 || arguments[0] != "--pipe" || arguments[2] != "--token" {
        return Err("不正な引数です。--help で使い方を表示します。".to_owned());
    }
    if arguments[1].is_empty() || arguments[3].is_empty() {
        return Err("pipe名とtokenは空にできません。".to_owned());
    }
    Ok(HelperArguments {
        pipe_name: arguments[1].clone(),
        token: arguments[3].clone(),
    })
}

fn run_helper(arguments: HelperArguments) -> Result<(), String> {
    let client = Arc::new(
        CapturePipeClient::connect(&arguments.pipe_name)
            .map_err(|error| error.to_string())?
            .handshake(&arguments.token)
            .map_err(|error| error.to_string())?,
    );
    let (shutdown_sender, shutdown_receiver) = bounded(1);
    let control_client = Arc::clone(&client);
    let control_thread = thread::Builder::new()
        .name("shortcut-helper-control".to_owned())
        .spawn(move || {
            // A clean shutdown message and an unexpected pipe break both end the helper.
            let _ = control_client.wait_for_shutdown();
            let _ = shutdown_sender.send(());
        })
        .map_err(|error| error.to_string())?;

    let (sender, receiver) = bounded(HELPER_QUEUE_CAPACITY);
    let keyboard_hook = start_keyboard_hook(sender.clone()).map_err(|error| error.to_string())?;
    let mouse_hook = match start_mouse_hook(sender) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = stop_keyboard_hook(keyboard_hook);
            return Err(error.to_string());
        }
    };

    let result = forward_inputs(&client, &receiver, &shutdown_receiver);
    stop_hooks(keyboard_hook, mouse_hook);
    drop(client);
    let _ = control_thread.join();
    result
}

fn forward_inputs(
    client: &CapturePipeClient,
    receiver: &Receiver<CapturedInput>,
    shutdown_receiver: &Receiver<()>,
) -> Result<(), String> {
    loop {
        select! {
            recv(shutdown_receiver) -> _ => return Ok(()),
            recv(receiver) -> received => match received {
                Ok(input) => client
                    .send_input(captured_input_to_message(input))
                    .map_err(|error| error.to_string())?,
                Err(_) => return Ok(()),
            }
        }
    }
}

fn stop_hooks(keyboard_hook: KeyboardHookHandle, mouse_hook: MouseHookHandle) {
    let _ = stop_keyboard_hook(keyboard_hook);
    let _ = stop_mouse_hook(mouse_hook);
}

fn captured_input_to_message(input: CapturedInput) -> InputMessage {
    InputMessage {
        protocol_version: PROTOCOL_VERSION,
        occurred_at: format_timestamp(input.occurred_at),
        ctrl: input.modifiers.ctrl,
        alt: input.modifiers.alt,
        win: input.modifiers.win,
        shift: input.modifiers.shift,
        main_input: match input.main_input {
            MainInput::Key(vk) => IpcMainInput::Key { vk },
            MainInput::MouseLeft => IpcMainInput::MouseLeft,
            MainInput::MouseRight => IpcMainInput::MouseRight,
            MainInput::MouseMiddle => IpcMainInput::MouseMiddle,
            MainInput::MouseX1 => IpcMainInput::MouseX1,
            MainInput::MouseX2 => IpcMainInput::MouseX2,
            MainInput::WheelUp => IpcMainInput::WheelUp,
            MainInput::WheelDown => IpcMainInput::WheelDown,
            MainInput::WheelLeft => IpcMainInput::WheelLeft,
            MainInput::WheelRight => IpcMainInput::WheelRight,
        },
        application: input.foreground_executable,
        pointer_x: input.pointer.map(|point| point.x),
        pointer_y: input.pointer.map(|point| point.y),
    }
}

fn format_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use shortcut_core::model::ModifierState;

    #[test]
    fn parses_pipe_and_token_arguments() {
        let arguments = vec![
            "--pipe".to_owned(),
            r"\\.\pipe\ShortcutKeyCounter-capture-test".to_owned(),
            "--token".to_owned(),
            "token".to_owned(),
        ];
        assert_eq!(
            parse_arguments(&arguments),
            Ok(HelperArguments {
                pipe_name: arguments[1].clone(),
                token: "token".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_missing_or_empty_helper_arguments() {
        assert!(parse_arguments(&[]).is_err());
        assert!(parse_arguments(&[
            "--pipe".to_owned(),
            "".to_owned(),
            "--token".to_owned(),
            "token".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn converts_captured_input_without_shortcut_text() {
        let input = CapturedInput {
            occurred_at: NaiveDate::from_ymd_opt(2026, 8, 15)
                .expect("date")
                .and_hms_milli_opt(9, 14, 21, 152)
                .expect("time"),
            modifiers: ModifierState {
                ctrl: true,
                alt: false,
                win: false,
                shift: true,
            },
            main_input: MainInput::Key(67),
            foreground_executable: "Code.exe".to_owned(),
            pointer: None,
        };
        let message = captured_input_to_message(input);
        assert_eq!(message.occurred_at, "2026-08-15 09:14:21.152");
        assert_eq!(message.main_input, IpcMainInput::Key { vk: 67 });
        assert_eq!(message.application, "Code.exe");
    }
}
