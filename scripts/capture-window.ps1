# Capture a MagpieCraft window to a PNG, for verification and documentation.
#
# Why this exists: an agent cannot look at a screen, and a GPU-rendered egui window is
# invisible to a naive screen grab when another window is in front. This script marks
# itself DPI aware (otherwise the bitmap is smaller than the window and the capture is
# clipped), finds the MagpieCraft window by title, and captures it with PrintWindow,
# falling back to a screen copy if PrintWindow returns a dark frame.
#
#   pwsh -File scripts/capture-window.ps1 -Key "lib:1|Neon Tales" -Name library-grid
#
# It never reads page content; it only records what the app painted.
[CmdletBinding()]
param(
    [string]$Exe = "",
    [string]$OutDir = "",
    [string]$Key = "",
    [string]$DataHome = "",
    [string]$Name = "browse",
    [string]$Keys = "",
    [int]$WaitSeconds = 12
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    public static string Title(IntPtr h) {
        int n = GetWindowTextLength(h);
        var sb = new StringBuilder(n + 2);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }
}
"@

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Exe) { $Exe = Join-Path $repo 'target\release\magpiecraft.exe' }
if (-not $OutDir) { $OutDir = Join-Path $repo 'docs\screenshots' }
if (-not (Test-Path $Exe)) { throw "app not built: $Exe (cargo build --release -p magpiecraft-app)" }

[void][Win32]::SetProcessDPIAware()
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$appArgs = @()
if ($DataHome) { $appArgs += @("--home", $DataHome) }
if ($Key) { $appArgs += @("--key", $Key) }
$argLine = ($appArgs | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
$proc = Start-Process -FilePath $Exe -ArgumentList $argLine -PassThru -WindowStyle Normal

$hwnd = [IntPtr]::Zero
$title = ""
$deadline = (Get-Date).AddSeconds(60)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 700
    $proc.Refresh()
    if ($proc.HasExited) { throw "the app exited during start-up (exit code $($proc.ExitCode))" }
    $candidate = $proc.MainWindowHandle
    if ($candidate -ne [IntPtr]::Zero) {
        $t = [Win32]::Title($candidate)
        if ($t -like "*MagpieCraft*") { $hwnd = $candidate; $title = $t; break }
    }
}
if ($hwnd -eq [IntPtr]::Zero) {
    Stop-Process -Id $proc.Id -Force
    throw "no MagpieCraft window appeared"
}

Start-Sleep -Seconds $WaitSeconds
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$winW = [Math]::Min(1240, $screen.Width - 40)
$winH = [Math]::Min(760, $screen.Height - 60)
[void][Win32]::ShowWindow($hwnd, 9)   # SW_RESTORE
[void][Win32]::SetWindowPos($hwnd, [IntPtr](-1), 20, 20, $winW, $winH, 0x0040)
$shell = New-Object -ComObject WScript.Shell
[void]$shell.AppActivate($proc.Id)
[void][Win32]::SetForegroundWindow($hwnd)
Start-Sleep -Seconds 2
if ($Keys -and ([Win32]::GetForegroundWindow() -eq $hwnd)) {
    foreach ($k in $Keys.ToCharArray()) {
        Start-Sleep -Milliseconds 350
        [void]$shell.SendKeys($k.ToString())
    }
    Start-Sleep -Seconds 2
}

$rect = New-Object Win32+RECT
[void][Win32]::GetWindowRect($hwnd, [ref]$rect)
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
if ($w -le 100 -or $h -le 100) {
    Stop-Process -Id $proc.Id -Force
    throw "window has no usable size: ${w}x${h}"
}

function Save-PrintWindow {
    param([IntPtr]$Handle, [int]$Width, [int]$Height, [string]$Path)
    $bmp = New-Object System.Drawing.Bitmap($Width, $Height)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $gfx.GetHdc()
    $ok = [Win32]::PrintWindow($Handle, $hdc, 2)   # PW_RENDERFULLCONTENT
    $gfx.ReleaseHdc($hdc)
    $gfx.Dispose()
    if (-not $ok) { $bmp.Dispose(); return $false }
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return $true
}

function Test-Brightness {
    param([string]$Path)
    $img = [System.Drawing.Bitmap]::FromFile($Path)
    $sum = 0
    $samples = 0
    for ($x = 10; $x -lt $img.Width; $x += [Math]::Max(1, [int]($img.Width / 12))) {
        for ($y = 10; $y -lt $img.Height; $y += [Math]::Max(1, [int]($img.Height / 12))) {
            $c = $img.GetPixel($x, $y)
            $sum += $c.R + $c.G + $c.B
            $samples++
        }
    }
    $img.Dispose()
    if ($samples -eq 0) { return 0 }
    return ($sum / $samples)
}

$file = Join-Path $OutDir ("{0}.png" -f $Name)
$method = "printwindow"
$ok = Save-PrintWindow -Handle $hwnd -Width $w -Height $h -Path $file
$brightness = if ($ok) { Test-Brightness -Path $file } else { 0 }
if (-not $ok -or $brightness -lt 6) {
    $method = "screen"
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $gfx.Dispose()
    $bmp.Dispose()
    $brightness = Test-Brightness -Path $file
}

$alive = -not $proc.HasExited
Stop-Process -Id $proc.Id -Force
Write-Output ("captured '{0}' {1}x{2} via {3} (brightness {4:N0}, app alive: {5}) to {6}" -f $title, $w, $h, $method, $brightness, $alive, $file)
