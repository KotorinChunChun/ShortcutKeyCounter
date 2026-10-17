param(
    [string]$Version = '0.1.5',
    [string]$TargetDir = (Join-Path $PSScriptRoot '..\..\target\release'),
    [string]$OutputDir = (Join-Path $PSScriptRoot "../../dev/history/v$Version/logs/package/Publish"),
    [string]$StageRoot = (Join-Path $PSScriptRoot "../../dev/history/v$Version/logs/package/staging")
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw '版はX.Y.Z形式で指定してください。' }
$ReleaseRoot = [IO.Path]::GetFullPath((Join-Path $Root 'release'))
$VersionLogsRoot = [IO.Path]::GetFullPath((Join-Path $Root "dev/history/v$Version/logs"))

function Resolve-ReleasePath {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    $fullPath = [IO.Path]::GetFullPath($Path)
    $allowed = @($ReleaseRoot, $VersionLogsRoot) | Where-Object {
        $prefix = $_.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
    }
    if (-not $allowed) {
        throw "配布物の出力先は対象版のlogsまたはrelease配下を指定してください: $fullPath"
    }
    return $fullPath
}

$OutputDir = Resolve-ReleasePath $OutputDir
$StageRoot = Resolve-ReleasePath $StageRoot
$MainExe = Join-Path $TargetDir 'ShortcutKeyCounter.exe'
$CaptureExe = Join-Path $TargetDir 'ShortcutKeyCapture.exe'
if (-not (Test-Path $MainExe)) { throw "Missing: $MainExe" }
if (-not (Test-Path $CaptureExe)) { throw "Missing: $CaptureExe" }

# 表示文の欠落や依存更新後の古い台帳を含む配布を防ぐ。
$ThirdParty = Join-Path $Root 'third-party'
$Manifest = Get-Content (Join-Path $ThirdParty 'manifest.json') -Raw | ConvertFrom-Json
if ((Get-FileHash (Join-Path $Root 'Cargo.lock') -Algorithm SHA256).Hash -ne $Manifest.cargoLockSha256) {
    throw 'Cargo.lock更新後に第三者ライセンス台帳を再生成してください。'
}
foreach ($required in @('THIRD-PARTY-NOTICES.txt', 'ASSET-PROVENANCE.md', 'webview2-sdk/LICENSE.txt', 'webview2-sdk/NOTICE.txt', 'rust-standard-library/COPYRIGHT-library.html')) {
    if (-not (Test-Path (Join-Path $ThirdParty $required))) { throw "ライセンス表示がありません: $required" }
}
foreach ($package in $Manifest.packages) {
    if ($package.bundledSource) {
        $source = Join-Path $ThirdParty $package.bundledSource
        if (-not (Test-Path $source) -or (Get-FileHash $source -Algorithm SHA256).Hash -ne $package.sourceSha256) {
            throw "同梱ソースが欠落または変更されています: $($package.name)"
        }
    }
}

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
Copy-Item $ThirdParty (Join-Path $Stage 'third-party') -Recurse
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip -CompressionLevel Optimal
Write-Host "ZIP: $Zip"
