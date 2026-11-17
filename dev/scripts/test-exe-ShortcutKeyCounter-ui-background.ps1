param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\target\debug\ShortcutKeyCounter.exe'),
    [string]$OutputDirectory,
    [int]$Port = 0
)
# 既存のDebug exeを隔離試験モードで非表示起動し、UIの主要操作をCLIから自動判定する。
# ウィンドウ・Overlayは表示せず、入力Hook・自動起動・本体管理窓口・実際のDownloadsも使わない。
# 前面表示や実入力が必要な項目（Overlay描画、Hook入力、トレイ操作、UAC）は対象外。
$ErrorActionPreference = 'Stop'
$ExePath = [System.IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path $ExePath)) {
    throw "ShortcutKeyCounter.exe が見つかりません: $ExePath`nbuild-debug-ShortcutKeyCounter-all.ps1 でDebugビルドしてください。"
}
# 隔離試験モードはDebugビルドだけで有効。Releaseを起動すると本番の保存先・入力Hookを使うため拒否する。
if ($ExePath -match '\\release\\') {
    throw "Release exeでは隔離試験モードが無効です。Debug exeを指定してください: $ExePath"
}

$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) { throw 'Node.js が見つかりません。Node.js 22.13以上を準備してください。' }
$nodeVersion = [version]((& $node.Source --version).TrimStart('v'))
if ($nodeVersion -lt [version]'22.13.0') {
    throw "Node.js 22.13以上が必要です（WebSocketとnode:sqliteを使用）。現在: $nodeVersion"
}

if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ('ShortcutKeyCounter-ui-background-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$isolatedRoot = Join-Path $OutputDirectory 'localappdata'
New-Item -ItemType Directory -Force -Path $isolatedRoot | Out-Null

if ($Port -eq 0) {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $Port = $listener.LocalEndpoint.Port
    $listener.Stop()
}

Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class BackgroundWindowProbe {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, StringBuilder name, int count);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out int value, int size);
}
'@
# 利用者に見えるウィンドウのクラス名を返す。Tauriの16x16透過メッセージ窓や
# 疑似コンソール(0x0)のように、表示属性はあっても画面に描かれないものは除く。
function Get-UserVisibleWindows([int]$ProcessId) {
    $script:userVisible = New-Object System.Collections.Generic.List[string]
    [BackgroundWindowProbe]::EnumWindows({ param($handle, $unused)
        [uint32]$owner = 0
        [void][BackgroundWindowProbe]::GetWindowThreadProcessId($handle, [ref]$owner)
        if ($owner -ne $ProcessId -or -not [BackgroundWindowProbe]::IsWindowVisible($handle)) { return $true }
        $cloaked = 0
        [void][BackgroundWindowProbe]::DwmGetWindowAttribute($handle, 14, [ref]$cloaked, 4)
        $rect = New-Object BackgroundWindowProbe+RECT
        [void][BackgroundWindowProbe]::GetWindowRect($handle, [ref]$rect)
        if ($cloaked -eq 0 -and ($rect.Right - $rect.Left) -gt 16 -and ($rect.Bottom - $rect.Top) -gt 16) {
            $name = New-Object System.Text.StringBuilder 256
            [void][BackgroundWindowProbe]::GetClassName($handle, $name, $name.Capacity)
            $script:userVisible.Add("$name $($rect.Right - $rect.Left)x$($rect.Bottom - $rect.Top)")
        }
        return $true
    }, [IntPtr]::Zero) | Out-Null
    return $script:userVisible
}
function Test-ForegroundOwnedBy([int]$ProcessId) {
    [uint32]$owner = 0
    [void][BackgroundWindowProbe]::GetWindowThreadProcessId([BackgroundWindowProbe]::GetForegroundWindow(), [ref]$owner)
    return $owner -eq $ProcessId
}

# 通常版の保存先が変更されないことを前後で比較する。
$productionData = Join-Path $env:LOCALAPPDATA 'ShortcutKeyCounter'
function Get-ProductionFingerprint {
    if (-not (Test-Path $productionData)) { return 'absent' }
    $latest = Get-ChildItem $productionData -Recurse -Force -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if ($latest) { return "$($latest.FullName)|$($latest.LastWriteTimeUtc.Ticks)" }
    return 'empty'
}
$productionBefore = Get-ProductionFingerprint

# 環境変数はこのpwshプロセスと起動するアプリだけに効く。
$env:SHORTCUT_KEY_COUNTER_TEST_LOCALAPPDATA = $isolatedRoot
$env:SHORTCUT_KEY_COUNTER_TEST_BACKGROUND_UI = '1'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port --remote-allow-origins=http://127.0.0.1:$Port"

Write-Host "[ShortcutKeyCounter] UI background test"
Write-Host "  exe    : $ExePath"
Write-Host "  output : $OutputDirectory"
# Debug exeはコンソールsubsystemのため、コンソールも非表示で起動する。
$process = Start-Process -FilePath $ExePath -WindowStyle Hidden -PassThru
$failures = New-Object System.Collections.Generic.List[string]
try {
    Start-Sleep -Seconds 3
    if ($process.HasExited) { throw "アプリが起動直後に終了しました（終了コード $($process.ExitCode)）。" }
    $visibleAtStart = Get-UserVisibleWindows $process.Id
    & $node.Source (Join-Path $PSScriptRoot 'test-exe-ShortcutKeyCounter-ui-background.mjs') $Port $isolatedRoot $OutputDirectory
    if ($LASTEXITCODE -ne 0) { $failures.Add("UI試験が失敗しました（終了コード $LASTEXITCODE）。") }

    $visible = @($visibleAtStart) + @(Get-UserVisibleWindows $process.Id) | Where-Object { $_ }
    if ($visible.Count -ne 0) { $failures.Add("ウィンドウが表示されています: $($visible -join ', ')") }
    else { Write-Host 'PASS  ウィンドウを表示していない（起動直後と試験後）' }
    if (Test-ForegroundOwnedBy $process.Id) { $failures.Add('アプリが前面ウィンドウを取得しました。') }
    else { Write-Host 'PASS  前面ウィンドウを取得していない' }
}
finally {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    # 隔離rootのprofileを使うWebView2子プロセスだけを停止する。
    Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
        Where-Object { $_.CommandLine -like "*$isolatedRoot*" } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

if ((Get-ProductionFingerprint) -ne $productionBefore) { $failures.Add("通常版の保存先が変更されました: $productionData") }
else { Write-Host 'PASS  通常版の保存先を変更していない' }

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "FAIL  $_" }
    Write-Host "[ShortcutKeyCounter] UI background test: FAILED (report: $OutputDirectory)"
    exit 1
}
Write-Host "[ShortcutKeyCounter] UI background test: PASSED (report: $OutputDirectory)"
