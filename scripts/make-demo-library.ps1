# Build a small synthetic demo library for screenshots and manual testing.
#
# Every page is generated here with System.Drawing: no copyrighted image is involved,
# and nothing is downloaded. Run it, then point the app at the folder.
#
#   pwsh -File scripts/make-demo-library.ps1
#   magpiecraft-mcp --home data/demo-home library-add Demo data/demo --kind comics
#   magpiecraft-mcp --home data/demo-home scan
[CmdletBinding()]
param([string]$Root = "")

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

if (-not $Root) {
    $repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
    $Root = Join-Path $repo 'data\demo'
}

function New-Page {
    param(
        [string]$Path,
        [int]$Page,
        [int]$Pages,
        [System.Drawing.Color]$A,
        [System.Drawing.Color]$B,
        [string]$Series,
        [string]$Volume
    )
    $w = 900
    $h = 1350
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $rect = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $A, $B, 45.0)
    $g.FillRectangle($brush, $rect)
    $brush.Dispose()
    # Panel frames, so a page reads as a comic page rather than a colour card.
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(50, 0, 0, 0), 6)
    for ($i = 0; $i -lt 3; $i++) {
        $g.DrawRectangle($pen, 80, (90 + $i * 400), 740, 340)
    }
    $pen.Dispose()
    $fontBig = New-Object System.Drawing.Font('Segoe UI', 64, [System.Drawing.FontStyle]::Bold)
    $fontSmall = New-Object System.Drawing.Font('Segoe UI', 30)
    $white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
    $shadow = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(120, 0, 0, 0))
    $g.DrawString($Series, $fontSmall, $shadow, 70, 1360 - 190)
    $g.DrawString($Volume, $fontSmall, $shadow, 70, 1360 - 140)
    $g.DrawString("page $Page of $Pages", $fontSmall, $shadow, 70, 1360 - 90)
    $g.DrawString("$Page", $fontBig, $white, ($w / 2 - 60), 40)
    $fontBig.Dispose()
    $fontSmall.Dispose()
    $white.Dispose()
    $shadow.Dispose()
    $g.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

function New-Volume {
    param([string]$Dir, [int]$Pages, $A, $B, [string]$Series, [string]$Volume)
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    for ($p = 1; $p -le $Pages; $p++) {
        New-Page -Path (Join-Path $Dir ("{0:D3}.png" -f $p)) -Page $p -Pages $Pages -A $A -B $B -Series $Series -Volume $Volume
    }
}

function New-Cbz {
    param([string]$Path, [int]$Pages, $A, $B, [string]$Series, [string]$Volume)
    $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("magpie-demo-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    for ($p = 1; $p -le $Pages; $p++) {
        New-Page -Path (Join-Path $stage ("{0:D3}.png" -f $p)) -Page $p -Pages $Pages -A $A -B $B -Series $Series -Volume $Volume
    }
    $number = $Volume -replace '[^0-9]', ''
    $comicInfo = @"
<?xml version="1.0" encoding="utf-8"?>
<ComicInfo>
  <Series>$Series</Series>
  <Number>$number</Number>
  <Title>$Volume</Title>
  <Writer>Synthetic Fixture</Writer>
  <Publisher>MagpieCraft Tests</Publisher>
  <Year>2026</Year>
  <PageCount>$Pages</PageCount>
  <LanguageISO>en</LanguageISO>
  <Genre>Demo</Genre>
</ComicInfo>
"@
    Set-Content -Path (Join-Path $stage 'ComicInfo.xml') -Value $comicInfo -Encoding utf8
    $zip = [System.IO.Path]::ChangeExtension($Path, '.zip')
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
    Move-Item -Path $zip -Destination $Path -Force
    Remove-Item -Recurse -Force $stage
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Root 'Neon Tales') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Root 'Forest Stories') | Out-Null

New-Cbz -Path (Join-Path $Root 'Neon Tales/Neon Tales v01 The Long Night.cbz') -Pages 12 `
    -A ([System.Drawing.Color]::FromArgb(255, 28, 22, 64)) -B ([System.Drawing.Color]::FromArgb(255, 214, 72, 122)) `
    -Series 'Neon Tales' -Volume 'v01 The Long Night'
New-Cbz -Path (Join-Path $Root 'Neon Tales/Neon Tales v02 Signal Lost.cbz') -Pages 10 `
    -A ([System.Drawing.Color]::FromArgb(255, 16, 48, 72)) -B ([System.Drawing.Color]::FromArgb(255, 92, 200, 214)) `
    -Series 'Neon Tales' -Volume 'v02 Signal Lost'
New-Cbz -Path (Join-Path $Root 'Neon Tales/Neon Tales v03 Static Bloom.cbz') -Pages 8 `
    -A ([System.Drawing.Color]::FromArgb(255, 40, 12, 48)) -B ([System.Drawing.Color]::FromArgb(255, 250, 168, 96)) `
    -Series 'Neon Tales' -Volume 'v03 Static Bloom'

New-Volume -Dir (Join-Path $Root 'Forest Stories/Vol 1 Moss and Rain') -Pages 9 `
    -A ([System.Drawing.Color]::FromArgb(255, 18, 44, 32)) -B ([System.Drawing.Color]::FromArgb(255, 128, 176, 96)) `
    -Series 'Forest Stories' -Volume 'Vol 1 Moss and Rain'
New-Volume -Dir (Join-Path $Root 'Forest Stories/Vol 2 The Clearing') -Pages 7 `
    -A ([System.Drawing.Color]::FromArgb(255, 60, 40, 18)) -B ([System.Drawing.Color]::FromArgb(255, 224, 196, 120)) `
    -Series 'Forest Stories' -Volume 'Vol 2 The Clearing'

$files = Get-ChildItem -Path $Root -Recurse -File
Write-Output ("demo library at {0}: {1} files, {2:N1} MB" -f $Root, $files.Count, (($files | Measure-Object Length -Sum).Sum / 1MB))
