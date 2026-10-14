param()
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$cargo = Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue
if (-not $cargo) {
    $scoopRoot = Join-Path $env:USERPROFILE 'scoop/apps/rustup-msvc/current'
    $candidate = Join-Path $scoopRoot '.cargo/bin/cargo.exe'
    if (-not (Test-Path -LiteralPath $candidate)) { throw 'cargoが見つかりません。Rust stable MSVCをPATHへ設定してください。' }
    $env:PATH = (Split-Path $candidate) + ';' + $env:PATH
    $env:CARGO_HOME = Join-Path $scoopRoot '.cargo'
    $env:RUSTUP_HOME = Join-Path $scoopRoot '.rustup'
    $cargo = Get-Command cargo -CommandType Application -ErrorAction Stop
}
Push-Location $projectRoot
try {
    & pwsh -ExecutionPolicy Bypass -File apps/shortcut-key-counter/scripts/build-ui.ps1
    if ($LASTEXITCODE) { throw 'ShortcutKeyCounterのUIビルドに失敗しました。' }
    & $cargo.Source build --locked -p shortcut-key-counter -p shortcut-key-capture
    if ($LASTEXITCODE) { throw 'ShortcutKeyCounterのDebugビルドに失敗しました。' }
} finally { Pop-Location }
Write-Output 'ShortcutKeyCounterのDebugビルド完了（起動・導入・公開は未実施）。'
