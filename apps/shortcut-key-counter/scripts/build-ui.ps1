$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$AppRoot = Split-Path -Parent $Here
$UiRoot = Join-Path $AppRoot 'ui'
$Dist = Join-Path $UiRoot 'dist'

$TsConfig = Join-Path $UiRoot 'tsconfig.json'

function Invoke-TypeScriptCompiler {
    $localTsc = Join-Path $UiRoot 'node_modules\.bin\tsc.cmd'
    if (Test-Path $localTsc) {
        Write-Host '[ShortcutKeyCounter] TypeScript: local node_modules compiler'
        & $localTsc -p $TsConfig
        return $LASTEXITCODE
    }

    $globalTsc = Get-Command tsc.cmd -ErrorAction SilentlyContinue
    if (-not $globalTsc) { $globalTsc = Get-Command tsc -ErrorAction SilentlyContinue }
    if ($globalTsc) {
        Write-Host '[ShortcutKeyCounter] TypeScript: global compiler'
        & $globalTsc.Source -p $TsConfig
        return $LASTEXITCODE
    }

    $npx = Get-Command npx.cmd -ErrorAction SilentlyContinue
    if (-not $npx) { $npx = Get-Command npx -ErrorAction SilentlyContinue }
    if ($npx) {
        Write-Host '[ShortcutKeyCounter] TypeScript: npx temporary compiler'
        & $npx.Source --yes --package typescript tsc -p $TsConfig
        return $LASTEXITCODE
    }

    $npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
    if (-not $npm) { $npm = Get-Command npm -ErrorAction SilentlyContinue }
    if ($npm) {
        Write-Host '[ShortcutKeyCounter] TypeScript: npm exec temporary compiler'
        & $npm.Source exec --yes --package=typescript -- tsc -p $TsConfig
        return $LASTEXITCODE
    }

    throw 'TypeScript compilerを起動できません。Node.jsに付属するnpm/npx、またはtscを利用可能にしてください。'
}

New-Item -ItemType Directory -Force -Path (Join-Path $Dist 'styles') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Dist 'js') | Out-Null
Copy-Item (Join-Path $UiRoot 'index.html') (Join-Path $Dist 'index.html') -Force
Copy-Item (Join-Path $UiRoot 'styles\main.css') (Join-Path $Dist 'styles\main.css') -Force
$tscExitCode = Invoke-TypeScriptCompiler
if ($tscExitCode -ne 0) { throw "TypeScript build failed: $tscExitCode" }
Write-Host '[ShortcutKeyCounter] UI build complete.'
