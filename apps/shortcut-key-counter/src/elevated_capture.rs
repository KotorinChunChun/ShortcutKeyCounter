//! 管理者権限Capture helperの起動・Named Pipe受信を管理する。
//!
//! v0.1.1でも通常権限Hookを停止せず、Elevated helperを追加Capture経路として併用する。
//! 同じ実入力が両経路から届いた場合は既存の共通DuplicateGuardが1秒以内の同一shortcutを
//! 1件へ抑制する。これによりhelper異常時にも通常アプリの計測が全面停止しない。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use shortcut_windows::elevation::{launch_helper_elevated, ElevationError};
use shortcut_windows::ipc::{CapturePipeServer, IpcError};
use thiserror::Error;

use crate::capture_health::{
    update_capture_health, CaptureHealthSnapshot, CaptureSource, HealthLevel, SharedCaptureHealth,
};
use crate::capture_worker::{CaptureGate, CaptureQueue};
use crate::diagnostics::{DiagnosticEvent, DiagnosticLogger};

#[derive(Debug, Error)]
pub enum ElevatedCaptureError {
    #[error("capture helperの起動に失敗しました: {0}")]
    Elevation(#[from] ElevationError),
    #[error("capture helperとのIPCに失敗しました: {0}")]
    Ipc(#[from] IpcError),
    #[error("elevated reader threadを起動できません: {0}")]
    ThreadStart(#[source] std::io::Error),
    #[error("ShortcutKeyCapture.exeが見つかりません: {0}")]
    HelperNotFound(PathBuf),
    #[error("管理者権限Captureとの接続がタイムアウトしました")]
    HandshakeTimeout,
}

pub struct ElevatedCaptureManager {
    server: Option<Arc<CapturePipeServer>>,
    stop_requested: Option<Arc<AtomicBool>>,
    reader: Option<JoinHandle<()>>,
}

impl ElevatedCaptureManager {
    pub fn new() -> Self {
        Self {
            server: None,
            stop_requested: None,
            reader: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.reader
            .as_ref()
            .is_some_and(|reader| !reader.is_finished())
    }

    fn discard_finished_reader(&mut self) {
        if self.reader.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
            self.server = None;
            self.stop_requested = None;
        }
    }

    /// Elevated helperを追加入力元として開始する。通常権限Hookは停止しない。
    pub fn start(
        &mut self,
        helper_path: impl AsRef<Path>,
        queue: CaptureQueue,
        capture_gate: CaptureGate,
        health: SharedCaptureHealth,
        logger: Arc<DiagnosticLogger>,
    ) -> Result<(), ElevatedCaptureError> {
        self.discard_finished_reader();
        if self.is_active() {
            return Ok(());
        }
        let helper_path = helper_path.as_ref();
        if !helper_path.is_file() {
            return Err(ElevatedCaptureError::HelperNotFound(
                helper_path.to_path_buf(),
            ));
        }

        let server = Arc::new(CapturePipeServer::new()?);
        if let Err(error) = launch_helper_elevated(helper_path, server.pipe_name(), server.token())
        {
            let _ = logger.record(DiagnosticEvent::HelperStartFailed);
            return Err(error.into());
        }
        let _ = logger.record(DiagnosticEvent::HelperStartSucceeded);

        // helperが起動失敗した場合にTauri commandを永久blockしないよう、acceptを別threadで待つ。
        let (accept_sender, accept_receiver) = mpsc::sync_channel(1);
        let accept_server = Arc::clone(&server);
        let _ = thread::Builder::new()
            .name("shortcut-elevated-accept".to_owned())
            .spawn(move || {
                let _ = accept_sender.send(accept_server.accept());
            })
            .map_err(ElevatedCaptureError::ThreadStart)?;

        let connection = match accept_receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => {
                let _ = logger.record(DiagnosticEvent::PipeDisconnected);
                return Err(error.into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                server.cancel_pending_io();
                server.disconnect();
                let _ = logger.record(DiagnosticEvent::PipeDisconnected);
                return Err(ElevatedCaptureError::HandshakeTimeout);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = logger.record(DiagnosticEvent::PipeDisconnected);
                return Err(ElevatedCaptureError::HandshakeTimeout);
            }
        };
        let _ = logger.record(DiagnosticEvent::PipeConnected);

        update_capture_health(
            &health,
            CaptureHealthSnapshot {
                source: CaptureSource::Elevated,
                level: HealthLevel::Ok,
                message: Some("通常権限Capture + 管理者権限Captureを併用中です。".to_owned()),
            },
        );

        let stop_requested = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_requested);
        let thread_capture_gate = Arc::clone(&capture_gate);
        let thread_health = Arc::clone(&health);
        let thread_logger = Arc::clone(&logger);
        let reader = thread::Builder::new()
            .name("shortcut-elevated-pipe-reader".to_owned())
            .spawn(move || loop {
                match connection.read_input() {
                    Ok(input) => {
                        if thread_stop.load(Ordering::Acquire) {
                            break;
                        }
                        if thread_capture_gate.load(Ordering::Acquire) {
                            let _ = queue.try_send_elevated(input);
                        }
                    }
                    Err(_) => {
                        let _ = thread_logger.record(DiagnosticEvent::PipeDisconnected);
                        if !thread_stop.load(Ordering::Acquire) {
                            update_capture_health(
                                &thread_health,
                                CaptureHealthSnapshot {
                                    source: CaptureSource::LocalFallback,
                                    level: HealthLevel::Degraded,
                                    message: Some(
                                        "管理者権限Captureとの接続が切れました。通常権限Captureは継続しています。"
                                            .to_owned(),
                                    ),
                                },
                            );
                        }
                        break;
                    }
                }
            })
            .map_err(ElevatedCaptureError::ThreadStart)?;

        self.server = Some(server);
        self.stop_requested = Some(stop_requested);
        self.reader = Some(reader);
        Ok(())
    }

    /// UIをブロックしないよう、Pipe shutdownとreader joinはcleanup threadへ退避する。
    pub fn stop(&mut self, health: SharedCaptureHealth) -> Result<(), ElevatedCaptureError> {
        self.discard_finished_reader();
        if let Some(stop_requested) = self.stop_requested.take() {
            stop_requested.store(true, Ordering::Release);
        }
        let server = self.server.take();
        let reader = self.reader.take();

        if server.is_some() || reader.is_some() {
            let _ = thread::Builder::new()
                .name("shortcut-elevated-cleanup".to_owned())
                .spawn(move || {
                    if let Some(server) = server {
                        // reader threadの同期ReadFileを明示cancelしてからPipeを切断する。
                        server.cancel_pending_io();
                        server.disconnect();
                    }
                    if let Some(reader) = reader {
                        let _ = reader.join();
                    }
                });
        }

        update_capture_health(
            &health,
            CaptureHealthSnapshot {
                source: CaptureSource::Local,
                level: HealthLevel::Ok,
                message: None,
            },
        );
        Ok(())
    }
}

impl Default for ElevatedCaptureManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ElevatedCaptureManager {
    fn drop(&mut self) {
        if let Some(stop_requested) = self.stop_requested.take() {
            stop_requested.store(true, Ordering::Release);
        }
        let server = self.server.take();
        let reader = self.reader.take();
        if server.is_some() || reader.is_some() {
            let _ = thread::Builder::new()
                .name("shortcut-elevated-drop-cleanup".to_owned())
                .spawn(move || {
                    if let Some(server) = server {
                        // reader threadの同期ReadFileを明示cancelしてからPipeを切断する。
                        server.cancel_pending_io();
                        server.disconnect();
                    }
                    if let Some(reader) = reader {
                        let _ = reader.join();
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manager_is_not_active() {
        assert!(!ElevatedCaptureManager::new().is_active());
    }
}
