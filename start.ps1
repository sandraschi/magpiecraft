# MagpieCraft launcher: builds the release binaries if they are missing, then starts the app.
# Double-click start.bat, or run: pwsh -File start.ps1
[CmdletBinding()]
param(
    [switch]$Cli,
    [string]$Home,
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

if ($Home) { $env:MAGPIECRAFT_HOME = $Home }

$app = Join-Path $root 'target\release\magpiecraft.exe'
$cli = Join-Path $root 'target\release\magpiecraft-mcp.exe'
if (-not (Test-Path $app) -or -not (Test-Path $cli)) {
    Write-Host 'Building MagpieCraft (first run takes a few minutes)...' -ForegroundColor Cyan
    cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($Cli) {
    & $cli status
    exit $LASTEXITCODE
}

if ($Smoke) {
    & $app --smoke
    exit $LASTEXITCODE
}

Write-Host 'Starting MagpieCraft...' -ForegroundColor Cyan
Start-Process -FilePath $app -WorkingDirectory $root
