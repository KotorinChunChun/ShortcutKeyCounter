param(
    [switch]$RunWebViewLifecycleTest,
    [string]$CargoTargetDirectory
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $Root

if ($CargoTargetDirectory) {
    New-Item -ItemType Directory -Force -Path $CargoTargetDirectory | Out-Null
    $env:CARGO_TARGET_DIR = (Resolve-Path $CargoTargetDirectory).Path
}
$ReleaseTargetDirectory = if ($env:CARGO_TARGET_DIR) {
    Join-Path $env:CARGO_TARGET_DIR 'release'
} else {
    Join-Path $Root 'target\release'
}

foreach ($command in @('cargo', 'rustc', 'node')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "$command が見つかりません。Rust stable と Node.js を準備してください。"
    }
}
# TypeScript compiler is resolved by build-ui.ps1 in this order:
# local node_modules/.bin/tsc -> global tsc -> npx/npm exec temporary package.
# Do not require a global TypeScript installation here.

& pwsh -ExecutionPolicy Bypass -File '.\apps\shortcut-key-counter\scripts\build-ui.ps1'
if ($LASTEXITCODE -ne 0) { throw 'UI build failed.' }

# Regression gate: Tauri command must bind MutexGuard before passing &Connection.
# Direct `&db_lock(&state)?` caused E0308 (expected Connection, found MutexGuard).
$tauriCommands = Join-Path $Root 'apps\shortcut-key-counter\src\tauri_commands.rs'
if (Select-String -Path $tauriCommands -SimpleMatch '&db_lock(&state)?' -Quiet) {
    throw 'tauri_commands.rs に禁止パターン &db_lock(&state)? が残っています。let db = db_lock(&state)?; の後に &db を渡してください。'
}

# Regression gate: Windows logon autostart must be tray-only via -hide.
$autostart = Join-Path $Root 'crates\shortcut-windows\src\autostart.rs'
if (-not (Select-String -Path $autostart -SimpleMatch 'pub const BACKGROUND_ARGUMENT: &str = "-hide";' -Quiet)) {
    throw '自動起動引数が -hide ではありません。Windowsログオン時はUIを生成しない仕様です。'
}

# Regression gate: elevated capture must not stop local hooks; local+elevated share the worker duplicate guard.
$elevatedCapture = Join-Path $Root 'apps\shortcut-key-counter\src\elevated_capture.rs'
if (Select-String -Path $elevatedCapture -SimpleMatch '.stop_capture()' -Quiet) {
    throw 'elevated_capture.rs が通常権限Hookを停止しています。Elevated helperは追加Capture経路として併用してください。'
}

# Regression gate: tray and WebView must share capture state immediately via Tauri event.
$traySource = Join-Path $Root 'apps\shortcut-key-counter\src\tray.rs'
$frontendApi = Join-Path $Root 'apps\shortcut-key-counter\ui\src\api.ts'
if (-not (Select-String -Path $traySource -SimpleMatch 'capture-state-changed' -Quiet) -or
    -not (Select-String -Path $frontendApi -SimpleMatch 'capture-state-changed' -Quiet)) {
    throw 'Capture状態のTray/WebView同期イベントが欠落しています。'
}

# Regression gate: second launch must be mutex-gated and activate Settings in the existing process.
$mainSource = Join-Path $Root 'apps\shortcut-key-counter\src\main.rs'
if (-not (Select-String -Path $mainSource -SimpleMatch 'acquire_or_notify_existing' -Quiet) -or
    -not (Select-String -Path $mainSource -SimpleMatch 'request_show_settings_window' -Quiet)) {
    throw 'Named Mutex単一起動または既存processの設定画面activationが欠落しています。'
}

# Regression gate: elevated stop must cancel pending pipe I/O and never synchronously join in AppState/Tauri command.
$ipcSource = Join-Path $Root 'crates\shortcut-windows\src\ipc.rs'
if (-not (Select-String -Path $ipcSource -SimpleMatch 'CancelIoEx' -Quiet) -or
    -not (Select-String -Path $elevatedCapture -SimpleMatch 'shortcut-elevated-cleanup' -Quiet)) {
    throw 'Elevated OFFのnonblocking Pipe cleanupが欠落しています。'
}

# Regression gate: windows 0.62 ShellExecuteW expects Option<HWND>.
$shellOpen = Join-Path $Root 'crates\shortcut-windows\src\shell_open.rs'
if (Select-String -Path $shellOpen -SimpleMatch 'HWND::default()' -Quiet) {
    throw 'shell_open.rs に旧ShellExecuteW呼び出し HWND::default() が残っています。第1引数は None または Some(HWND) を使用してください。'
}

# Regression gate: do not append comparison directly to a tail-position unsafe block.
# `unsafe { WaitForSingleObject(...) } == WAIT_OBJECT_0` is parsed incorrectly in this position.
$singleInstance = Join-Path $Root 'crates\shortcut-windows\src\single_instance.rs'
if (Select-String -Path $singleInstance -SimpleMatch 'unsafe { WaitForSingleObject(event, timeout_ms) } == WAIT_OBJECT_0' -Quiet) {
    throw 'single_instance.rs に不正なWaitForSingleObject比較式が残っています。戻り値をlet変数へ束縛してからWAIT_OBJECT_0と比較してください。'
}

# Regression gate: public/product naming must be ShortcutKeyCounter everywhere relevant.
$tauriConfig = Join-Path $Root 'apps\shortcut-key-counter\tauri.conf.json'
$mainCargo = Join-Path $Root 'apps\shortcut-key-counter\Cargo.toml'
if (-not (Select-String -Path $tauriConfig -SimpleMatch '"productName": "ShortcutKeyCounter"' -Quiet) -or
    -not (Select-String -Path $tauriConfig -SimpleMatch '"mainBinaryName": "ShortcutKeyCounter"' -Quiet) -or
    -not (Select-String -Path $mainCargo -SimpleMatch 'name = "ShortcutKeyCounter"' -Quiet)) {
    throw 'アプリ名またはEXE名が ShortcutKeyCounter に統一されていません。'
}

# Regression gate: realtime log refresh must be event-driven, not timer-driven.
$realtimeLog = Join-Path $Root 'apps\shortcut-key-counter\src\realtime_log.rs'
$frontendMain = Join-Path $Root 'apps\shortcut-key-counter\ui\src\main.ts'
if (-not (Select-String -Path $realtimeLog -SimpleMatch 'pub fn subscribe(&self)' -Quiet) -or
    -not (Select-String -Path $frontendApi -SimpleMatch 'realtime-log-changed' -Quiet) -or
    -not (Select-String -Path $frontendMain -SimpleMatch 'onRealtimeLogChanged' -Quiet)) {
    throw 'リアルタイムログの入力検知イベント更新が欠落しています。'
}
if (Select-String -Path $frontendMain -SimpleMatch "setInterval(() => { if (activePage === 'realtime-log')" -Quiet) {
    throw 'リアルタイムログに旧タイマーポーリングが残っています。'
}

# Regression gate: dark/light theme must be persisted through SettingsSnapshot.
$settingsSource = Join-Path $Root 'apps\shortcut-key-counter\src\settings.rs'
$frontendTypes = Join-Path $Root 'apps\shortcut-key-counter\ui\src\types.ts'
if (-not (Select-String -Path $settingsSource -SimpleMatch 'pub fn set_theme' -Quiet) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'pub fn set_theme' -Quiet) -or
    -not (Select-String -Path $frontendTypes -SimpleMatch "export type Theme = 'dark' | 'light'" -Quiet)) {
    throw 'ダーク/ライトテーマ設定が欠落しています。'
}

# v0.1.3 Overlay regression gate: native Win32 overlay, configurable timing/stack and capture ordering.
$overlayConfig = Join-Path $Root 'apps\shortcut-key-counter\src\overlay\config.rs'
$overlayManager = Join-Path $Root 'apps\shortcut-key-counter\src\overlay\manager.rs'
$overlayWindow = Join-Path $Root 'crates\shortcut-windows\src\overlay_window.rs'
$captureWorker = Join-Path $Root 'apps\shortcut-key-counter\src\capture_worker.rs'
if (-not (Select-String -Path $overlayConfig -SimpleMatch 'display_duration_ms' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'enter_ms: 160' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'exit_ms: 400' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'max_rows' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'OverlayPositionMode' -Quiet)) {
    throw 'v0.1.3 Overlayの表示時間・アニメーション・行数・位置設定が欠落しています。'
}
if (-not (Select-String -Path $overlayManager -SimpleMatch 'SOURCE_DEDUP_WINDOW: Duration = Duration::from_millis(50)' -Quiet) -or
    -not (Select-String -Path $elevatedCapture -SimpleMatch 'try_send_elevated' -Quiet)) {
    throw 'OverlayのLocal/Elevated 50ms source dedupまたはElevated source routingが欠落しています。'
}
if (-not (Select-String -Path $overlayWindow -SimpleMatch 'WS_EX_NOACTIVATE' -Quiet) -or
    -not (Select-String -Path $overlayWindow -SimpleMatch 'WM_NCHITTEST' -Quiet) -or
    -not (Select-String -Path $overlayWindow -SimpleMatch 'HTTRANSPARENT' -Quiet)) {
    throw 'Overlay Windowのnon-activate/click-through設定が欠落しています。'
}
$keyPolicy = Join-Path $Root 'apps\shortcut-key-counter\src\key_policy.rs'
if (-not (Select-String -Path $keyPolicy -SimpleMatch 'ContinueDisplay' -Quiet) -or
    -not (Select-String -Path $keyPolicy -SimpleMatch 'RecordAndDisplayBreak' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'OverlayInputKind::SingleKey' -Quiet) -or
    -not (Select-String -Path $traySource -SimpleMatch 'overlay-enabled-changed' -Quiet) -or
    -not (Select-String -Path $frontendApi -SimpleMatch 'overlay-enabled-changed' -Quiet)) {
    throw 'v0.1.3の単独キー表示ポリシーまたはOverlay ON/OFF同期が欠落しています。'
}

# Follow-up overlay/input gate: target selection, fixed monitor, full modifier matrix and middle click.
$overlayTypes = Join-Path $Root 'apps\shortcut-key-counter\ui\src\types.ts'
$overlayUi = Join-Path $Root 'apps\shortcut-key-counter\ui\src\main.ts'
$overlayHtml = Join-Path $Root 'apps\shortcut-key-counter\ui\index.html'
if (-not (Select-String -Path $overlayConfig -SimpleMatch 'OverlayDisplayMode' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'OverlayMonitorMode' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'fixed_monitor_id' -Quiet) -or
    -not (Select-String -Path $overlayManager -SimpleMatch 'append_or_create_continuation' -Quiet) -or
    -not (Select-String -Path $overlayWindow -SimpleMatch 'available_monitors' -Quiet) -or
    -not (Select-String -Path $overlayWindow -SimpleMatch 'set_display_target' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'MouseMiddle' -Quiet) -or
    -not (Select-String -Path $overlayTypes -SimpleMatch 'OverlayDisplayMode' -Quiet) -or
    -not (Select-String -Path $overlayUi -SimpleMatch 'keyPolicyMatrixHead' -Quiet) -or
    -not (Select-String -Path $overlayHtml -SimpleMatch 'overlayDisplayMode' -Quiet)) {
    throw 'オーバーレイ表示先・固定モニター・全修飾パターン表・センタークリックの実装が欠落しています。'
}
$captureWorkerText = [IO.File]::ReadAllText($captureWorker)
$overlayIndex = $captureWorkerText.IndexOf('overlay.try_show(')
$duplicateIndex = $captureWorkerText.IndexOf('duplicate_guard.should_accept')
if ($overlayIndex -lt 0 -or $duplicateIndex -lt 0 -or $overlayIndex -gt $duplicateIndex) {
    throw 'Overlay通知はDB用1秒DuplicateGuardより前でなければなりません。'
}
# v0.1.3 hard security / database-environment regression gates.
$securityGate = Join-Path $Root 'apps\shortcut-key-counter\src\security_gate.rs'
$dbEnvironment = Join-Path $Root 'apps\shortcut-key-counter\src\database_environment.rs'
$sampleData = Join-Path $Root 'apps\shortcut-key-counter\src\sample_data.rs'
if (-not (Select-String -Path $captureWorker -SimpleMatch 'RULE-SEC-001 / RULE-SEC-006' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'is_unmodified_recording_prohibited(vk)' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'trace.outcome == ProcessOutcome::Recorded' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'SecurityGate::new' -Quiet) -or
    -not (Test-Path $securityGate)) {
    throw 'Security Gate、修飾なし英数記号入力、またはRealtime Logのhard invariantが欠落しています。'
}
if (-not (Select-String -Path $captureWorker -SimpleMatch 'MainInput::WheelLeft' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'MainInput::MouseX1' -Quiet) -or
    -not (Select-String -Path $overlayManager -SimpleMatch 'allowed_positions' -Quiet) -or
    -not (Select-String -Path $overlayManager -SimpleMatch 'preview_sample' -Quiet)) {
    throw 'v0.1.3 horizontal wheel/X buttons/allowed positions/SAMPLE previewが欠落しています。'
}
if (-not (Test-Path $dbEnvironment) -or -not (Test-Path $sampleData) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'switch_database_environment' -Quiet) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'generate_sample_data' -Quiet)) {
    throw 'v0.1.3 Production/Test DB切替またはsample data機能が欠落しています。'
}

# v0.1.4 regression gates: runtime settings, analytics, key-search suppression, repeat compressor and 60-day samples.
$analyticsSource = Join-Path $Root 'apps\shortcut-key-counter\src\analytics.rs'
$keySearchSource = Join-Path $Root 'apps\shortcut-key-counter\src\key_search.rs'
$settingsRepository = Join-Path $Root 'crates\shortcut-storage\src\repositories\settings_repository.rs'
$overlayRenderer = Join-Path $Root 'apps\shortcut-key-counter\src\overlay\renderer.rs'
if (-not (Select-String -Path $settingsRepository -SimpleMatch 'duplicate_guard_seconds' -Quiet) -or
    -not (Select-String -Path $settingsRepository -SimpleMatch 'detail_event_retention_days' -Quiet) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'duplicate_guard_threshold' -Quiet)) {
    throw 'v0.1.4 DuplicateGuard/retention runtime設定が欠落しています。'
}
if (-not (Test-Path $analyticsSource) -or
    -not (Select-String -Path $analyticsSource -SimpleMatch 'pub fn rankings' -Quiet) -or
    -not (Select-String -Path $analyticsSource -SimpleMatch 'pub fn usage_graph' -Quiet) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'get_rankings_v014' -Quiet) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'get_usage_graph_v014' -Quiet)) {
    throw 'v0.1.4 統合ランキング/使用率グラフAPIが欠落しています。'
}
if (-not (Test-Path $keySearchSource) -or
    -not (Select-String -Path $captureWorker -SimpleMatch 'key_search.consume_if_armed(&input)' -Quiet) -or
    -not (Select-String -Path $tauriCommands -SimpleMatch 'begin_key_search' -Quiet)) {
    throw 'v0.1.4 キーマッピング物理キー検索の完全消費経路が欠落しています。'
}
if (-not (Select-String -Path $captureWorker -SimpleMatch 'overlay.reset_session()' -Quiet) -or
    -not (Select-String -Path $overlayManager -SimpleMatch 'OverlayCommand::ResetSession' -Quiet)) {
    throw 'v0.1.4 Security Gate遷移時のOverlay session/repeat run破棄が欠落しています。'
}
if (-not (Select-String -Path $overlayManager -SimpleMatch 'OVERLAY_REPEAT_GAP: Duration = Duration::from_millis(700)' -Quiet) -or
    -not (Select-String -Path $overlayManager -SimpleMatch 'repeat_count' -Quiet) -or
    -not (Select-String -Path $overlayRenderer -SimpleMatch 'render_repeat_badge' -Quiet) -or
    -not (Select-String -Path $overlayConfig -SimpleMatch 'repeat_counter' -Quiet)) {
    throw 'v0.1.4 Overlay連打圧縮/カウンターテーマが欠落しています。'
}
if (-not (Select-String -Path $sampleData -SimpleMatch 'Duration::days(59)' -Quiet) -or
    -not (Select-String -Path $sampleData -Pattern 'for\s+\w+\s+in\s+0\.\.60' -Quiet) -or
    -not (Select-String -Path $sampleData -SimpleMatch 'previous_day_realtime_samples' -Quiet)) {
    throw 'v0.1.4 60日サンプル/前日Realtime Log sampleが欠落しています。'
}
if (-not (Select-String -Path $frontendMain -SimpleMatch 'loadRanking' -Quiet) -or
    -not (Select-String -Path $frontendMain -SimpleMatch 'drawUsageGraph' -Quiet) -or
    -not (Select-String -Path $overlayHtml -SimpleMatch 'repeatCounterPreview' -Quiet)) {
    throw 'v0.1.4 UIモックv3準拠のランキング/グラフ/連打設定UIが欠落しています。'
}

$overlayModuleText = (Get-ChildItem (Join-Path $Root 'apps\shortcut-key-counter\src\overlay') -Filter '*.rs' | ForEach-Object { Get-Content $_.FullName -Raw }) -join "`n"
if ($overlayModuleText -match 'WebviewWindow|WebView2|WebviewWindowBuilder') {
    throw 'v0.1.1 Overlay subsystemへWebView依存が混入しています。Native Win32 Overlayを維持してください。'
}

& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'cargo fmt check failed.' }
& cargo check --locked --workspace --all-targets --all-features
if ($LASTEXITCODE -ne 0) { throw 'cargo check failed.' }
& cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'cargo clippy failed.' }
& cargo test --locked --workspace --all-features
if ($LASTEXITCODE -ne 0) { throw 'cargo test failed.' }
& cargo build --locked --release -p shortcut-key-counter -p shortcut-key-capture
if ($LASTEXITCODE -ne 0) { throw 'cargo release build failed.' }

if ($RunWebViewLifecycleTest) {
    & pwsh -ExecutionPolicy Bypass -File '.\dev\scripts\test-exe-ShortcutKeyCounter-webview-lifecycle.ps1' -ExePath (Join-Path $ReleaseTargetDirectory 'ShortcutKeyCounter.exe')
    if ($LASTEXITCODE -ne 0) { throw 'WebView lifecycle test failed.' }
}

Write-Host '[ShortcutKeyCounter] release quality gate completed.'
