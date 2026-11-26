# MagpieCraft launcher: builds the release binaries if they are missing, then starts the app.
# Double-click start.bat, or run: pwsh -File start.ps1
# NOTE: keep local variable names away from parameter names ($cliExe, not $cli:
# PowerShell variables are case-insensitive, so $cli collides with the [switch]$Cli
# parameter and binding fails. $Home is worse: it is a read-only automatic variable,
# so the home override is called $DataHome.
param(
    [switch]$Cli,
    [string]$DataHome,
    [switch]$Smoke
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

function Require-Command {
    param([string]$Name, [string]$Hint)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Write-Host "Missing required command: $Name" -ForegroundColor Red
        Write-Host "  $Hint" -ForegroundColor Yellow
        exit 1
    }
}

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Require-Command cargo "Install Rust from https://rustup.rs and reopen this window."

if ($DataHome) { $env:MAGPIECRAFT_HOME = $DataHome }

$appExe = Join-Path $root 'target\release\magpiecraft.exe'
$cliExe = Join-Path $root 'target\release\magpiecraft-mcp.exe'
# Rebuild when the binaries are missing or older than the sources, so a
# double-click never launches a stale build after an update.
$needBuild = (-not (Test-Path $appExe) -or -not (Test-Path $cliExe))
if (-not $needBuild) {
    $exeTime = (Get-Item $appExe).LastWriteTime
    $newer = Get-ChildItem -Path (Join-Path $root 'crates') -Recurse -Include '*.rs','Cargo.toml' |
        Where-Object { $_.LastWriteTime -gt $exeTime } |
        Select-Object -First 1
    if ($newer) {
        Write-Host ("Sources changed since last build ({0}), rebuilding..." -f $newer.FullName) -ForegroundColor Cyan
        $needBuild = $true
    }
}
if ($needBuild) {
    Write-Host 'Building MagpieCraft (first run takes a few minutes)...' -ForegroundColor Cyan
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($Cli) {
    & $cliExe status
    exit $LASTEXITCODE
}

if ($Smoke) {
    & $appExe --smoke
    exit $LASTEXITCODE
}

Write-Host 'Starting MagpieCraft...' -ForegroundColor Cyan
Start-Process -FilePath $appExe -WorkingDirectory $root
