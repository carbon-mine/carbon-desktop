# SPDX-License-Identifier: MPL-2.0
<#
.SYNOPSIS
Captures deterministic native README screenshots from carbon-desktop-gpui.

.DESCRIPTION
Launches the GPUI launcher with disposable dev fixtures, waits for the real
CARBON_LIBRARY_READY marker, forces wide/compact window sizes, captures the
window pixels, and writes a manifest with hashes and metadata. The script
hard-fails on blank/uniform captures and on unstable captures (two consecutive
grabs differ).
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDir,
    [Parameter(Mandatory)][string]$ManifestPath,
    [string]$CommitSha = '',
    [string]$SourceRef = '',
    [int]$ReadyTimeoutSeconds = 120,
    [int]$SettleMilliseconds = 1500
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms

$uia = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$conditionTrue = [System.Windows.Automation.Condition]::TrueCondition

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;

public static class CarbonCaptureNative {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

    private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);

    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, int dwAttribute, out RECT pvAttribute, int cbAttribute);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);

    public const uint SWP_NOZORDER = 0x0004;
    public const uint SWP_NOACTIVATE = 0x0010;
    public const int SW_RESTORE = 9;
    public const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
    public const uint PW_RENDERFULLCONTENT = 0x00000002;

    public static IntPtr FindMainWindow(uint processId, string titleContains) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, lParam) => {
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            if (pid != processId) return true;
            if (!IsWindowVisible(hwnd)) return true;
            var sb = new StringBuilder(512);
            GetWindowText(hwnd, sb, sb.Capacity);
            var title = sb.ToString();
            if (string.IsNullOrEmpty(title)) return true;
            if (title.IndexOf(titleContains, StringComparison.OrdinalIgnoreCase) >= 0) {
                found = hwnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static Bitmap Capture(IntPtr hwnd) {
        RECT rect;
        int hr = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, out rect, Marshal.SizeOf(typeof(RECT)));
        if (hr != 0) {
            GetWindowRect(hwnd, out rect);
        }
        int width = Math.Max(1, rect.Right - rect.Left);
        int height = Math.Max(1, rect.Bottom - rect.Top);
        var bmp = new Bitmap(width, height, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            bool ok = false;
            try {
                ok = PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT);
            } finally {
                g.ReleaseHdc(hdc);
            }
            if (!ok) {
                g.Clear(Color.Transparent);
                IntPtr windowDc = GetWindowDC(hwnd);
                if (windowDc == IntPtr.Zero) throw new InvalidOperationException("GetWindowDC failed");
                try {
                    IntPtr hdc2 = g.GetHdc();
                    try {
                        const int SRCCOPY = 0x00CC0020;
                        ok = BitBlt(hdc2, 0, 0, width, height, windowDc, 0, 0, SRCCOPY);
                    } finally {
                        g.ReleaseHdc(hdc2);
                    }
                } finally {
                    ReleaseDC(hwnd, windowDc);
                }
                if (!ok) throw new InvalidOperationException("PrintWindow and BitBlt capture failed");
            }
        }
        return bmp;
    }

    [DllImport("gdi32.dll")] private static extern bool BitBlt(IntPtr hdcDest, int nXDest, int nYDest, int nWidth, int nHeight, IntPtr hdcSrc, int nXSrc, int nYSrc, int dwRop);
}
'@ -ReferencedAssemblies System.Drawing.dll

function Wait-Until([scriptblock]$Condition, [string]$Message, [int]$TimeoutSeconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $value = & $Condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 200
    }
    throw $Message
}

function Get-WindowRectObject([IntPtr]$Hwnd) {
    $rect = New-Object CarbonCaptureNative+RECT
    [void][CarbonCaptureNative]::GetWindowRect($Hwnd, [ref]$rect)
    [pscustomobject]@{
        left = $rect.Left; top = $rect.Top; right = $rect.Right; bottom = $rect.Bottom;
        width = [Math]::Max(0, $rect.Right - $rect.Left)
        height = [Math]::Max(0, $rect.Bottom - $rect.Top)
    }
}

function Set-WindowSize([IntPtr]$Hwnd, [int]$Width, [int]$Height) {
    [void][CarbonCaptureNative]::ShowWindow($Hwnd, [CarbonCaptureNative]::SW_RESTORE)
    [void][CarbonCaptureNative]::BringWindowToTop($Hwnd)
    [void][CarbonCaptureNative]::SetForegroundWindow($Hwnd)
    $ok = [CarbonCaptureNative]::SetWindowPos($Hwnd, [IntPtr]::Zero, 50, 50, $Width, $Height, [CarbonCaptureNative]::SWP_NOZORDER)
    if (-not $ok) { throw "SetWindowPos failed for ${Width}x${Height}" }
    Start-Sleep -Milliseconds 500
    $actual = Get-WindowRectObject $Hwnd
    # Try once more if the window manager clamped it far off.
    if ([Math]::Abs($actual.width - $Width) -gt 40 -or [Math]::Abs($actual.height - $Height) -gt 40) {
        [void][CarbonCaptureNative]::SetWindowPos($Hwnd, [IntPtr]::Zero, 50, 50, $Width, $Height, [CarbonCaptureNative]::SWP_NOZORDER)
        Start-Sleep -Milliseconds 500
    }
}

function Get-BitmapStats([System.Drawing.Bitmap]$Bmp) {
    $w = $Bmp.Width
    $h = $Bmp.Height
    $rect = [System.Drawing.Rectangle]::new(0, 0, $w, $h)
    $data = $Bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = [Math]::Abs($data.Stride)
        $bytes = New-Object byte[] ($stride * $h)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
    } finally {
        $Bmp.UnlockBits($data)
    }
    # Sample pixels for variance/unique count (every 16th pixel in each axis).
    $set = [System.Collections.Generic.HashSet[int]]::new()
    $count = 0L
    $sum = 0.0
    $sumSq = 0.0
    $stepY = 16
    $stepX = 16
    for ($y = 0; $y -lt $h; $y += $stepY) {
        $row = $y * $stride
        for ($x = 0; $x -lt $w; $x += $stepX) {
            $i = $row + ($x * 4)
            $b = [int]$bytes[$i]
            $g = [int]$bytes[$i + 1]
            $r = [int]$bytes[$i + 2]
            # Ignore alpha for visual uniqueness, but keep it for blank check by using RGB.
            $rgb = ($r -shl 16) -bor ($g -shl 8) -bor $b
            [void]$set.Add($rgb)
            $lum = (0.2126 * $r) + (0.7152 * $g) + (0.0722 * $b)
            $sum += $lum
            $sumSq += $lum * $lum
            $count++
        }
    }
    $mean = if ($count -gt 0) { $sum / $count } else { 0 }
    $variance = if ($count -gt 0) { ($sumSq / $count) - ($mean * $mean) } else { 0 }
    [pscustomobject]@{
        uniqueRgbSampleCount = $set.Count
        sampleCount = $count
        luminanceMean = [Math]::Round($mean, 4)
        luminanceVariance = [Math]::Round($variance, 4)
    }
}

function Get-FileSha256([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Save-Capture([IntPtr]$Hwnd, [string]$Path) {
    $bmp = [CarbonCaptureNative]::Capture($Hwnd)
    try {
        $dir = Split-Path -Parent $Path
        [IO.Directory]::CreateDirectory($dir) | Out-Null
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bmp.Dispose()
    }
}

function Assert-CaptureValid([string]$Path, [string]$Label) {
    $bmp = [System.Drawing.Bitmap]::FromFile($Path)
    try {
        $stats = Get-BitmapStats $bmp
        $w = $bmp.Width
        $h = $bmp.Height
    } finally {
        $bmp.Dispose()
    }
    if ($w -lt 100 -or $h -lt 100) {
        throw "${Label}: capture too small ${w}x${h}"
    }
    # Blank/uniform detection: very few distinct sampled colors or near-zero luminance variance.
    if ($stats.uniqueRgbSampleCount -lt 8) {
        throw "${Label}: capture appears blank/uniform (uniqueRgbSampleCount=$($stats.uniqueRgbSampleCount), variance=$($stats.luminanceVariance), mean=$($stats.luminanceMean))"
    }
    if ($stats.luminanceVariance -lt 1.0) {
        throw "${Label}: capture appears blank/uniform (uniqueRgbSampleCount=$($stats.uniqueRgbSampleCount), variance=$($stats.luminanceVariance), mean=$($stats.luminanceMean))"
    }
    return $stats
}

function Invoke-StableCapture([IntPtr]$Hwnd, [string]$Path, [string]$Label) {
    $first = "$Path.first.png"
    $second = "$Path.second.png"
    if (Test-Path $first) { Remove-Item -LiteralPath $first -Force }
    if (Test-Path $second) { Remove-Item -LiteralPath $second -Force }
    Save-Capture $Hwnd $first
    Start-Sleep -Milliseconds 350
    Save-Capture $Hwnd $second
    $hash1 = Get-FileSha256 $first
    $hash2 = Get-FileSha256 $second
    $stats1 = Assert-CaptureValid $first $Label
    $stats2 = Assert-CaptureValid $second "$Label (second grab)"
    if ($hash1 -ne $hash2) {
        throw "${Label}: capture changed between two consecutive grabs (stale/unstable detection). first=$hash1 second=$hash2"
    }
    Move-Item -LiteralPath $first -Destination $Path -Force
    Remove-Item -LiteralPath $second -Force
    $size = (Get-Item -LiteralPath $Path).Length
    $bmp = [System.Drawing.Bitmap]::FromFile($Path)
    try { $w = $bmp.Width; $h = $bmp.Height } finally { $bmp.Dispose() }
    [pscustomobject]@{
        path = $Path
        sha256 = $hash1
        bytes = [long]$size
        width = $w
        height = $h
        stats = $stats1
        secondGrabSha256 = $hash2
        stable = $true
    }
}

function Find-UiaElement([System.Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $cond = [System.Windows.Automation.PropertyCondition]::new($uia::NameProperty, $Name)
    $Root.FindFirst($scope::Descendants, $cond)
}

function Find-UiaElementLike([System.Windows.Automation.AutomationElement]$Root, [string]$Pattern) {
    $all = $Root.FindAll($scope::Descendants, $conditionTrue)
    for ($i = 0; $i -lt $all.Count; $i++) {
        $n = $all.Item($i).Current.Name
        if ($n -and $n -like $Pattern) { return $all.Item($i) }
    }
    return $null
}

function Invoke-UiaElement([System.Windows.Automation.AutomationElement]$Element) {
    if ($null -eq $Element) { throw 'Cannot invoke null UIA element' }
    $invokeRef = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invokeRef)) {
        $p = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $p.Invoke()
        return
    }
    $selRef = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selRef)) {
        $p = $Element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
        $p.Select()
        return
    }
    try { $Element.SetFocus() } catch { }
    throw "Element '$($Element.Current.Name)' has no Invoke or SelectionItem pattern"
}

function Wait-UiaName([System.Windows.Automation.AutomationElement]$Root, [string]$Name, [int]$TimeoutSeconds = 30) {
    Wait-Until { Find-UiaElement $Root $Name } "UIA element not found: $Name" $TimeoutSeconds
}

$exe = (Resolve-Path -LiteralPath $Executable).Path
$out = [IO.Path]::GetFullPath($OutputDir)
$manifestOut = [IO.Path]::GetFullPath($ManifestPath)
[IO.Directory]::CreateDirectory($out) | Out-Null
[IO.Directory]::CreateDirectory((Split-Path -Parent $manifestOut)) | Out-Null

if (-not $CommitSha) {
    try { $CommitSha = (& git -C (Split-Path -Parent $exe) rev-parse HEAD 2>$null) } catch { }
    if (-not $CommitSha) { $CommitSha = 'unknown' }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("carbon-readme-screenshots-" + [Guid]::NewGuid().ToString('N'))
$fixtureRoot = Join-Path $work 'fixture-root'
$readyFile = Join-Path $work 'ready.txt'
[IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null

$process = $null
try {
    $psi = [Diagnostics.ProcessStartInfo]::new($exe)
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = Split-Path -Parent $exe
    $psi.ArgumentList.Add('--seed-fixtures')
    $psi.ArgumentList.Add('default')
    $psi.ArgumentList.Add('--fixture-root')
    $psi.ArgumentList.Add($fixtureRoot)
    $psi.ArgumentList.Add('--ready-file')
    $psi.ArgumentList.Add($readyFile)
    $psi.Environment['GPUI_DISABLE_DIRECT_COMPOSITION'] = '1'
    $psi.Environment['CARBON_README_SCREENSHOT'] = '1'
    $process = [Diagnostics.Process]::Start($psi)

    $deadline = [DateTime]::UtcNow.AddSeconds($ReadyTimeoutSeconds)
    $ready = $false
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($process.HasExited) {
            throw "Launcher exited before CARBON_LIBRARY_READY (exit code $($process.ExitCode))"
        }
        if (Test-Path -LiteralPath $readyFile) {
            $text = Get-Content -LiteralPath $readyFile -Raw -ErrorAction SilentlyContinue
            if ($text -match 'CARBON_LIBRARY_READY\s+\d+') { $ready = $true; break }
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $ready) {
        throw "CARBON_LIBRARY_READY marker not observed within $ReadyTimeoutSeconds seconds"
    }

    $hwnd = Wait-Until {
        if ($process.HasExited) { throw "Launcher exited while waiting for window" }
        $h = [CarbonCaptureNative]::FindMainWindow([uint32]$process.Id, 'Carbon Desktop')
        if ($h -ne [IntPtr]::Zero) { $h } else { $null }
    } 'Main window not found' 60

    [void][CarbonCaptureNative]::ShowWindow($hwnd, [CarbonCaptureNative]::SW_RESTORE)
    [void][CarbonCaptureNative]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 1000

    $root = Wait-Until {
        try { $uia::FromHandle($hwnd) } catch { $null }
    } 'UIA root not available for main window' 30

    $captures = New-Object System.Collections.Generic.List[object]

    function Capture-State([string]$Id, [string]$Alt, [int]$Width, [int]$Height, [scriptblock]$Prepare) {
        & $Prepare
        Set-WindowSize $hwnd $Width $Height
        Start-Sleep -Milliseconds $SettleMilliseconds
        $rect = Get-WindowRectObject $hwnd
        $file = Join-Path $out ($Id + '.png')
        $cap = Invoke-StableCapture $hwnd $file $Id
        $captures.Add([ordered]@{
            id = $Id
            file = (Split-Path -Leaf $file)
            alt = $Alt
            width = $cap.width
            height = $cap.height
            bytes = $cap.bytes
            sha256 = $cap.sha256
            windowRect = [ordered]@{ left = $rect.left; top = $rect.top; right = $rect.right; bottom = $rect.bottom; width = $rect.width; height = $rect.height }
            stats = [ordered]@{
                uniqueRgbSampleCount = $cap.stats.uniqueRgbSampleCount
                sampleCount = $cap.stats.sampleCount
                luminanceMean = $cap.stats.luminanceMean
                luminanceVariance = $cap.stats.luminanceVariance
            }
            stableTwoGrab = $cap.stable
            secondGrabSha256 = $cap.secondGrabSha256
        }) | Out-Null
        Write-Host "Captured $Id $($cap.width)x$($cap.height) sha256=$($cap.sha256)"
    }

    # library-wide: navigate to Library tab/surface in a wide window.
    Capture-State -Id 'library-wide' -Alt 'Carbon Desktop library showing local instances in a wide window' -Width 1280 -Height 860 -Prepare {
        $library = Find-UiaElement $root 'Library'
        if ($null -ne $library) {
            Invoke-UiaElement $library
            Start-Sleep -Milliseconds 500
        }
    }

    # library-compact: compact library list.
    Capture-State -Id 'library-compact' -Alt 'Carbon Desktop library list in a compact window' -Width 520 -Height 900 -Prepare {
        $library = Find-UiaElement $root 'Library'
        if ($null -eq $library) { $library = Find-UiaElementLike $root '*Library*' }
        if ($null -ne $library) {
            Invoke-UiaElement $library
            Start-Sleep -Milliseconds 500
        }
        # If a detail/back button is present, go back to list.
        $back = Find-UiaElementLike $root '*Back to library*'
        if ($null -ne $back) {
            Invoke-UiaElement $back
            Start-Sleep -Milliseconds 500
        }
    }

    # instance-compact: select a known instance in compact view.
    Capture-State -Id 'instance-compact' -Alt 'Carbon Desktop selected instance details in a compact window' -Width 520 -Height 900 -Prepare {
        $library = Find-UiaElement $root 'Library'
        if ($null -ne $library) { Invoke-UiaElement $library; Start-Sleep -Milliseconds 300 }
        # Prefer Vanilla survival row if present; otherwise first instance-like element.
        $target = Find-UiaElement $root 'Vanilla survival'
        if ($null -eq $target) {
            # Try to find by common profile names in default fixtures.
            foreach ($name in @('Vanilla survival, 26.3', 'Vanilla survival')) {
                $target = Find-UiaElement $root $name
                if ($target) { break }
            }
        }
        if ($null -eq $target) {
            # Fallback: click any element whose name contains "Vanilla survival".
            $all = $root.FindAll($scope::Descendants, $conditionTrue)
            for ($i = 0; $i -lt $all.Count; $i++) {
                $n = $all.Item($i).Current.Name
                if ($n -and $n -like '*Vanilla survival*') { $target = $all.Item($i); break }
            }
        }
        if ($null -eq $target) { throw 'Could not find Vanilla survival instance for instance-compact capture' }
        Invoke-UiaElement $target
        Start-Sleep -Milliseconds 700
    }

    $manifest = [ordered]@{
        schema = 'carbon-readme-screenshots/v1'
        generatedAt = [DateTime]::UtcNow.ToString('o')
        commit = $CommitSha
        sourceRef = $SourceRef
        repository = 'carbon-mine/carbon-desktop'
        executable = $exe
        seed = [ordered]@{
            fixtures = 'default'
            fixtureRoot = $fixtureRoot
            readyFile = $readyFile
            readyMarker = 'CARBON_LIBRARY_READY'
        }
        screenshots = @($captures)
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestOut -Encoding utf8
    Write-Host "Wrote manifest $manifestOut"
}
finally {
    if ($null -ne $process -and -not $process.HasExited) {
        try { $process.CloseMainWindow() | Out-Null } catch { }
        Start-Sleep -Milliseconds 500
        if (-not $process.HasExited) {
            try { $process.Kill() } catch { }
        }
    }
    if ($null -ne $process) {
        try { $process.Dispose() } catch { }
    }
    if (Test-Path -LiteralPath $work) {
        try { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue } catch { }
    }
}
