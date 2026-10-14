param(
    [string]$Version = '0.1.4',
    [string]$TargetDir = (Join-Path $PSScriptRoot '..\..\target\release'),
    [string]$OutputDir = (Join-Path $PSScriptRoot '..\..\release\Publish'),
    [string]$StageRoot = (Join-Path $PSScriptRoot '..\..\release\staging')
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ReleaseRoot = [IO.Path]::GetFullPath((Join-Path $Root 'release'))

function Resolve-ReleasePath {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    $fullPath = [IO.Path]::GetFullPath($Path)
    $releasePrefix = $ReleaseRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($releasePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "配布物の出力先は release ディレクトリ配下を指定してください: $fullPath"
    }
    return $fullPath
}

$OutputDir = Resolve-ReleasePath $OutputDir
$StageRoot = Resolve-ReleasePath $StageRoot
$MainExe = Join-Path $TargetDir 'ShortcutKeyCounter.exe'
$CaptureExe = Join-Path $TargetDir 'ShortcutKeyCapture.exe'
if (-not (Test-Path $MainExe)) { throw "Missing: $MainExe" }
if (-not (Test-Path $CaptureExe)) { throw "Missing: $CaptureExe" }

$PackageName = "ShortcutKeyCounter-v$Version-win-x64"
$Stage = Resolve-ReleasePath (Join-Path $StageRoot $PackageName)
$Zip = Resolve-ReleasePath (Join-Path $OutputDir "$PackageName.zip")
Remove-Item $Stage -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $Zip -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Copy-Item $MainExe (Join-Path $Stage 'ShortcutKeyCounter.exe')
Copy-Item $CaptureExe (Join-Path $Stage 'ShortcutKeyCapture.exe')
Copy-Item (Join-Path $Root 'README.txt') (Join-Path $Stage 'README.txt')
Copy-Item (Join-Path $Root 'LICENSE.txt') (Join-Path $Stage 'LICENSE.txt')
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip -CompressionLevel Optimal
Write-Host "ZIP: $Zip"
