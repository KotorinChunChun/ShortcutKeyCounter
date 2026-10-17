param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$Fixture = Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) ('license-fixture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force "$Fixture/dev/scripts", "$Fixture/target/release" | Out-Null
Copy-Item "$Root/dev/scripts/package-ShortcutKeyCounter-zip.ps1" "$Fixture/dev/scripts/"
Copy-Item "$Root/third-party" "$Fixture/third-party" -Recurse
foreach ($file in @('Cargo.lock','LICENSE.txt','README.txt')) { Copy-Item "$Root/$file" "$Fixture/$file" }
# 起動しないダミーで、パッケージ処理のみを検証する。製品ビルドの証拠ではない。
Set-Content "$Fixture/target/release/ShortcutKeyCounter.exe" '配布試験用ダミー・実行不可'
Set-Content "$Fixture/target/release/ShortcutKeyCapture.exe" '配布試験用ダミー・実行不可'
& "$Fixture/dev/scripts/package-ShortcutKeyCounter-zip.ps1"
$zip = [IO.Compression.ZipFile]::OpenRead("$Fixture/release/Publish/ShortcutKeyCounter-v0.1.4-win-x64.zip")
try {
    $entries = @($zip.Entries.FullName.Replace('\','/'))
    foreach ($required in @('third-party/THIRD-PARTY-NOTICES.txt','third-party/ASSET-PROVENANCE.md','third-party/webview2-sdk/NOTICE.txt')) {
        if ($entries -notcontains $required) { throw "ZIPに不足: $required" }
    }
    if (@($entries | Where-Object { $_ -like 'third-party/sources/*.crate' }).Count -ne 5) { throw 'MPL原本が5件ではありません。' }
} finally { $zip.Dispose() }
function Expect-Rejection([string]$Label) {
    $rejected = $false
    try { & "$Fixture/dev/scripts/package-ShortcutKeyCounter-zip.ps1" } catch { $rejected = $true }
    if (-not $rejected) { throw "拒否されませんでした: $Label" }
    Write-Output "PASS: $Label"
}
Add-Content "$Fixture/Cargo.lock" '# 台帳陳腐化の試験'
Expect-Rejection 'Cargo.lockと台帳の不一致'
Copy-Item "$Root/Cargo.lock" "$Fixture/Cargo.lock" -Force
$source = Get-ChildItem "$Fixture/third-party/sources/*.crate" | Select-Object -First 1
[IO.File]::WriteAllText($source.FullName, '原本改変の試験')
Expect-Rejection 'MPL原本の改変'
Copy-Item "$Root/third-party/sources/$($source.Name)" $source.FullName -Force
Remove-Item -LiteralPath "$Fixture/third-party/webview2-sdk/NOTICE.txt"
Expect-Rejection '必要なNOTICEの欠落'
Write-Output 'PASS: ZIPの表示文・素材出典・MPL原本5件の同梱'
