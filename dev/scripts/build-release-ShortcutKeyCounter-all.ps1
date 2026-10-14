param(
    [string]$CargoTargetDirectory
)

$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $Root

if ($CargoTargetDirectory) {
    New-Item -ItemType Directory -Force -Path $CargoTargetDirectory | Out-Null
    $env:CARGO_TARGET_DIR = (Resolve-Path $CargoTargetDirectory).Path
}

foreach ($command in @('cargo', 'rustc', 'node')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "$command が見つかりません。Rust stable と Node.js を準備してください。"
    }
}

& pwsh -ExecutionPolicy Bypass -File '.\apps\shortcut-key-counter\scripts\build-ui.ps1'
if ($LASTEXITCODE -ne 0) { throw 'UI build failed.' }

& cargo build --release -p shortcut-key-counter -p shortcut-key-capture
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }

Write-Host '[ShortcutKeyCounter] release build completed.'
