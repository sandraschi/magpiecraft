# One-off verification: open Settings, wheel-scroll down, capture.
# Same PrintWindow technique as capture-window.ps1.
param([string]$Name = "settings-classifier", [string]$Key = "lib:1|")
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win32b {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, int dwData, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder text, int count);
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc e, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    public static string Title(IntPtr hWnd) {
        int n = GetWindowTextLength(hWnd);
        var sb = new StringBuilder(n + 2);
        GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    public const uint WHEEL = 0x0800;
}
"@
$repo = "D:\Dev\repos\magpiecraft"
$exe = Join-Path $repo 'target\release\magpiecraft.exe'
$proc = Start-Process -FilePath $exe -ArgumentList "--page","settings","--key",$Key -PassThru -WindowStyle Normal
$hwnd = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds(60)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 700
    $proc.Refresh()
    if ($proc.HasExited) { throw "app exited during startup" }
    $found = @{ h = [IntPtr]::Zero }
    $pidWant = $proc.Id
    $cb = [Win32b+EnumProc]{
        param([IntPtr]$h, [IntPtr]$p)
        $pidOut = [uint32]0
        [void][Win32b]::GetWindowThreadProcessId($h, [ref]$pidOut)
        if ($pidOut -eq $pidWant -and [Win32b]::Title($h) -like '*MagpieCraft*') {
            $found.h = $h
            return $false
        }
        return $true
    }.GetNewClosure()
    [void][Win32b]::EnumWindows($cb, [IntPtr]::Zero)
    if ($found.h -ne [IntPtr]::Zero) { $hwnd = $found.h; break }
}
if ($hwnd -eq [IntPtr]::Zero) { Stop-Process -Id $proc.Id -Force; throw "no window" }
Start-Sleep -Seconds 8
[void][Win32b]::ShowWindow($hwnd, 9)
[void][Win32b]::SetWindowPos($hwnd, [IntPtr](-1), 20, 20, 1240, 760, 0x0040)
[void][Win32b]::SetForegroundWindow($hwnd)
$shell = New-Object -ComObject WScript.Shell
[void]$shell.AppActivate($proc.Id)
[void][Win32b]::SetForegroundWindow($hwnd)
Start-Sleep -Seconds 2
$rect = New-Object Win32b+RECT
[void][Win32b]::GetWindowRect($hwnd, [ref]$rect)
for ($i = 0; $i -lt 6 -and ([Win32b]::GetForegroundWindow() -ne $hwnd); $i++) {
    [void][Win32b]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 800
}
$fg = [Win32b]::GetForegroundWindow()
$n = [Win32b]::GetWindowTextLength($fg)
$sb = New-Object System.Text.StringBuilder($n + 2)
[void][Win32b]::GetWindowText($fg, $sb, $sb.Capacity)
Write-Output ("diag fg_title='{0}'" -f $sb.ToString())
Write-Output ("diag rect={0},{1}-{2}x{3} visible={4} iconic={5} fg_is_app={6}" -f $rect.Left,$rect.Top,$rect.Right,$rect.Bottom,[Win32b]::IsWindowVisible($hwnd),[Win32b]::IsIconic($hwnd),($fg -eq $hwnd))
$cx = [int](($rect.Left + $rect.Right) / 2)
$cy = [int](($rect.Top + $rect.Bottom) / 2)
[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point($cx, $cy)
Start-Sleep -Milliseconds 500
for ($i = 0; $i -lt 2; $i++) {
    [Win32b]::mouse_event([Win32b]::WHEEL, 0, 0, -240, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
Start-Sleep -Seconds 2
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$gfx = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $gfx.GetHdc()
$ok = [Win32b]::PrintWindow($hwnd, $hdc, 2)
$gfx.ReleaseHdc($hdc)
$gfx.Dispose()
if (-not $ok) {
    $bmp.Dispose()
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx2 = [System.Drawing.Graphics]::FromImage($bmp)
    [void][Win32b]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 800
    $gfx2.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
    $gfx2.Dispose()
}
$path = Join-Path $repo ("docs\screenshots\{0}.png" -f $Name)
$bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
$alive = -not $proc.HasExited
Stop-Process -Id $proc.Id -Force
Write-Output "saved $path alive=$alive ok=$ok"
