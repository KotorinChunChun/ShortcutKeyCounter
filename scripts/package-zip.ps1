param(
    [string]$Version = '0.1.4',
    [string]$TargetDir = (Join-Path $PSScriptRoot '..\target\release'),
    [string]$OutputDir = (Join-Path $PSScriptRoot '..\dist')
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$MainExe = Join-Path $TargetDir 'ShortcutKeyCounter.exe'
$CaptureExe = Join-Path $TargetDir 'ShortcutKeyCapture.exe'
if (-not (Test-Path $MainExe)) { throw "Missing: $MainExe" }
if (-not (Test-Path $CaptureExe)) { throw "Missing: $CaptureExe" }

$PackageName = "ShortcutKeyCounter-v$Version-win-x64"
$Stage = Join-Path $OutputDir $PackageName
$Zip = Join-Path $OutputDir "$PackageName.zip"
Remove-Item $Stage -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $Zip -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item $MainExe (Join-Path $Stage 'ShortcutKeyCounter.exe')
Copy-Item $CaptureExe (Join-Path $Stage 'ShortcutKeyCapture.exe')
Copy-Item (Join-Path $Root 'README.txt') (Join-Path $Stage 'README.txt')
Copy-Item (Join-Path $Root 'LICENSE.txt') (Join-Path $Stage 'LICENSE.txt')
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip -CompressionLevel Optimal
Write-Host "ZIP: $Zip"
