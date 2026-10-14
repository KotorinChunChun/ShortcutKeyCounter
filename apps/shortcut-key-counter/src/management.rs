//! 本体との管理契約。入力・統計・設定の内容を管理応答へ含めない。
use crate::app_state::AppState;
use crate::window_manager::show_main_window;
use serde::{Deserialize, Serialize};
use shortcut_windows::management_pipe::ManagementPipe;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

pub const TOOL_ID: &str = "shortcut-key-counter";

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    command: String,
    #[serde(default)]
    expected_pid: Option<u32>,
    #[serde(default)]
    request_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reply<'a> {
    protocol_version: u32,
    request_id: &'a str,
    pid: u32,
    ready: bool,
    version: &'static str,
    launch_origin: &'a str,
    capabilities: [&'static str; 3],
    result: &'static str,
    detail: &'static str,
}

fn parse_request(line: &str) -> Result<Request, serde_json::Error> {
    if line == "status" {
        return Ok(Request {
            command: "status".into(),
            ..Default::default()
        });
    }
    serde_json::from_str(line)
}

fn validate(request: &Request, pid: u32, blocked: bool) -> (&'static str, &'static str) {
    if request.expected_pid.is_some_and(|expected| expected != pid) {
        return ("refused", "対象PIDが変わりました。");
    }
    if request.request_id.len() > 128 {
        return ("refused", "要求IDが長すぎます。");
    }
    match request.command.as_str() {
        "show" | "stop" if request.expected_pid.is_none() => {
            ("refused", "操作には対象PIDが必要です。")
        }
        "status" | "show" => ("completed", ""),
        "stop" if blocked => (
            "refused",
            "未保存のキーマッピング編集があります。保存または破棄してから停止してください。",
        ),
        "stop" => ("accepted", ""),
        _ => ("unsupported", "未対応の操作です。"),
    }
}

pub fn launch_origin(arguments: &[String]) -> &str {
    arguments
        .windows(2)
        .find(|pair| pair[0] == "--launch-origin")
        .map(|pair| pair[1].as_str())
        .filter(|value| matches!(*value, "via_kotorintoys" | "direct_task" | "host_manual"))
        .unwrap_or("standalone")
}

pub fn start(app: AppHandle, origin: String) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("shortcut-kotorintoys-management".into())
        .spawn(move || {
            // SID/ACLが確立できない場合、権限を緩めた窓口は作らない。
            let Ok(pipe) = ManagementPipe::create(TOOL_ID) else {
                return;
            };
            while !app.state::<AppState>().is_exiting() {
                if pipe.accept(Duration::from_millis(250)).is_err() {
                    continue;
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                let accepted = handle(&pipe, &app, &origin, deadline).unwrap_or(false);
                pipe.wait_for_client_close(deadline);
                pipe.disconnect();
                if accepted {
                    let stop_app = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        let state = stop_app.state::<AppState>();
                        // 応答と停止の間に編集を開始した場合も、未保存データを守る。
                        if !state.mapping_edit_is_blocking() {
                            state.mark_exiting();
                            stop_app.exit(0);
                        }
                    });
                }
            }
        })
        .map(|_| ())
}

fn handle(
    pipe: &ManagementPipe,
    app: &AppHandle,
    origin: &str,
    deadline: Instant,
) -> windows::core::Result<bool> {
    if !pipe.same_session()? {
        return Ok(false);
    }
    let line = pipe.read_line(deadline)?;
    let Ok(request) = parse_request(&line) else {
        return Ok(false);
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let ui = app.clone();
    app.run_on_main_thread(move || {
        if Instant::now() < deadline {
            let _ = sender.send(ui.state::<AppState>().mapping_edit_is_blocking());
        }
    })
    .map_err(|_| {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))
    })?;
    let Ok(blocked) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    else {
        return Ok(false);
    };
    let (mut result, mut detail) = validate(&request, std::process::id(), blocked);
    if result == "completed" && request.command == "show" {
        // WebView生成はUIスレッド以外から行い、実際のshow/focusの完了後に応答する。
        if Instant::now() >= deadline || show_main_window(app, None).is_err() {
            result = "failed";
            detail = "画面を表示できませんでした。";
        }
    }
    let reply = Reply {
        protocol_version: 2,
        request_id: &request.request_id,
        pid: std::process::id(),
        ready: true,
        version: env!("CARGO_PKG_VERSION"),
        launch_origin: origin,
        capabilities: ["status", "show", "stop"],
        result,
        detail,
    };
    let line = serde_json::to_string(&reply).expect("固定管理応答");
    pipe.write_line(&line, deadline)?;
    Ok(result == "accepted")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_json_requests_are_compatible() {
        assert_eq!(parse_request("status").unwrap().command, "status");
        let req =
            parse_request(r#"{"command":"show","expectedPid":42,"requestId":"request"}"#).unwrap();
        assert_eq!(req.expected_pid, Some(42));
        assert_eq!(req.request_id, "request");
        assert!(parse_request("{").is_err());
    }
    #[test]
    fn mutations_require_the_current_pid_and_respect_unsaved_edits() {
        let mut req = Request {
            command: "stop".into(),
            expected_pid: Some(42),
            ..Default::default()
        };
        assert_eq!(validate(&req, 41, false).0, "refused");
        assert_eq!(validate(&req, 42, true).0, "refused");
        assert_eq!(validate(&req, 42, false).0, "accepted");
        req.expected_pid = None;
        assert_eq!(validate(&req, 42, false).0, "refused");
        req.command = "shortcuts".into();
        assert_eq!(validate(&req, 42, false).0, "unsupported");
        req.request_id = "x".repeat(129);
        assert_eq!(validate(&req, 42, false).0, "refused");
    }
    #[test]
    fn origin_is_not_guessed_from_unknown_arguments() {
        assert_eq!(
            launch_origin(&["--launch-origin".into(), "host_manual".into()]),
            "host_manual"
        );
        assert_eq!(
            launch_origin(&["--launch-origin".into(), "unknown".into()]),
            "standalone"
        );
        assert_eq!(launch_origin(&["--launch-origin".into()]), "standalone");
    }
}
