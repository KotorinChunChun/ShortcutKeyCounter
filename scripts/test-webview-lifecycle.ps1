param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\target\release\ShortcutKeyCounter.exe'),
    [int]$TimeoutSeconds = 20
)
$ErrorActionPreference = 'Stop'
$ExePath = [System.IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path $ExePath)) { throw "ShortcutKeyCounter.exe not found: $ExePath" }

Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WindowProbe {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
}
'@

function Get-MainWindow([int]$Pid) {
    $script:__found = [IntPtr]::Zero
    [WindowProbe]::EnumWindows({ param($h,$l)
        [uint32]$windowPid = 0
        [void][WindowProbe]::GetWindowThreadProcessId($h, [ref]$windowPid)
        if ($windowPid -eq $Pid -and [WindowProbe]::IsWindowVisible($h)) {
            $sb = New-Object System.Text.StringBuilder 256
            [void][WindowProbe]::GetWindowText($h, $sb, $sb.Capacity)
            if ($sb.ToString() -eq 'ShortcutKeyCounter') { $script:__found = $h; return $false }
        }
        return $true
    }, [IntPtr]::Zero) | Out-Null
    $found = $script:__found
    $script:__found = [IntPtr]::Zero
    return $found
}

function Wait-Until([scriptblock]$Condition, [string]$Description) {
    $limit = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $limit)
    throw "Timeout: $Description"
}

function Get-Descendants([int]$RootPid) {
    $all = @(Get-CimInstance Win32_Process)
    $ids = New-Object 'System.Collections.Generic.HashSet[int]'
    [void]$ids.Add($RootPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($p in $all) {
            if ($ids.Contains([int]$p.ParentProcessId) -and -not $ids.Contains([int]$p.ProcessId)) {
                [void]$ids.Add([int]$p.ProcessId); $changed = $true
            }
        }
    }
    return @($all | Where-Object { $ids.Contains([int]$_.ProcessId) -and [int]$_.ProcessId -ne $RootPid })
}

function Get-WebViewChildren([int]$RootPid) {
    return @(Get-Descendants $RootPid | Where-Object { $_.Name -ieq 'msedgewebview2.exe' })
}

function Get-WebViewPrivateBytes([int]$RootPid) {
    $total = [int64]0
    foreach ($child in (Get-WebViewChildren $RootPid)) {
        $process = Get-Process -Id ([int]$child.ProcessId) -ErrorAction SilentlyContinue
        if ($process) { $total += [int64]$process.PrivateMemorySize64 }
    }
    return $total
}

function Request-UiBySecondLaunch {
    $second = Start-Process -FilePath $ExePath -PassThru
    Wait-Until { $second.HasExited } 'second instance handoff'
}

Write-Host '[1/8] Launch with -hide; WebView must not exist'
$process = Start-Process -FilePath $ExePath -ArgumentList '-hide' -PassThru
$rootPid = $process.Id
try {
    Start-Sleep -Seconds 2
    if ((Get-MainWindow $rootPid) -ne [IntPtr]::Zero) { throw 'main window exists immediately after startup' }
    if ((Get-WebViewChildren $rootPid).Count -ne 0) { throw 'WebView2 exists immediately after tray-only startup' }
    Write-Host '  PASS: backend/tray started without main window or WebView2 child'

    Write-Host '[2/8] Launch second instance without parameters; existing process must show UI/settings'
    Request-UiBySecondLaunch
    Wait-Until { (Get-MainWindow $rootPid) -ne [IntPtr]::Zero } 'main window creation'
    Wait-Until { (Get-WebViewChildren $rootPid).Count -gt 0 } 'WebView2 child process creation'
    $firstUiBytes = Get-WebViewPrivateBytes $rootPid
    Write-Host "  PASS: UI/WebView2 lazily created; children = $((Get-WebViewChildren $rootPid).Count), private bytes = $firstUiBytes"

    Write-Host '[3/8] Close main window with WM_CLOSE'
    $hwnd = Get-MainWindow $rootPid
    if (-not [WindowProbe]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'WM_CLOSE failed' }
    Wait-Until { (Get-MainWindow $rootPid) -eq [IntPtr]::Zero } 'main window destruction'
    if ((Get-Process -Id $rootPid -ErrorAction SilentlyContinue) -eq $null) { throw 'backend process exited when UI was closed' }
    Write-Host '  PASS: backend process is still alive and native main window is gone'

    Write-Host '[4/8] Verify WebView2 processes are released'
    Wait-Until { (Get-WebViewChildren $rootPid).Count -eq 0 } 'WebView2 child process release'
    Write-Host '  PASS: no msedgewebview2.exe descendants remain; WebView2 private bytes = 0'

    Write-Host '[5/8] Recreate UI after a full destroy'
    Request-UiBySecondLaunch
    Wait-Until { (Get-MainWindow $rootPid) -ne [IntPtr]::Zero } 'main window recreation'
    Wait-Until { (Get-WebViewChildren $rootPid).Count -gt 0 } 'WebView2 recreation'
    $secondUiBytes = Get-WebViewPrivateBytes $rootPid
    Write-Host "  PASS: UI/WebView2 recreated; children = $((Get-WebViewChildren $rootPid).Count), private bytes = $secondUiBytes"

    Write-Host '[6/8] Destroy recreated UI and verify release is repeatable'
    $hwnd = Get-MainWindow $rootPid
    [void][WindowProbe]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    Wait-Until { (Get-MainWindow $rootPid) -eq [IntPtr]::Zero } 'second main window destruction'
    Wait-Until { (Get-WebViewChildren $rootPid).Count -eq 0 } 'second WebView2 release'
    Write-Host '  PASS: repeated destroy/release succeeded'

    Write-Host '[7/8] Verify normal first launch creates UI'
    Stop-Process -Id $rootPid -Force -ErrorAction SilentlyContinue
    Wait-Until { (Get-Process -Id $rootPid -ErrorAction SilentlyContinue) -eq $null } 'tray-only process stop'
    $normal = Start-Process -FilePath $ExePath -PassThru
    $rootPid = $normal.Id
    Wait-Until { (Get-MainWindow $rootPid) -ne [IntPtr]::Zero } 'normal startup main window'
    Wait-Until { (Get-WebViewChildren $rootPid).Count -gt 0 } 'normal startup WebView2'
    Write-Host '  PASS: normal launch shows UI without -hide'

    Write-Host '[8/8] RESULT: PASS'
    Write-Host 'Physical tray left-button double-click uses the same show_main_window path; verify that gesture once during manual acceptance. Second-launch activation targets the Settings page.'
}
finally {
    Stop-Process -Id $rootPid -Force -ErrorAction SilentlyContinue
}
