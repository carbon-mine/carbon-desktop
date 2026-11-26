param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    [switch]$ExerciseKeyboardAccelerator
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
$uia = [System.Windows.Automation.AutomationElement]
$scope = [System.Windows.Automation.TreeScope]
$everything = [System.Windows.Automation.Condition]::TrueCondition
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class GpuiTestWindows {
    private delegate bool Visitor(IntPtr window, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Visitor visitor, IntPtr unused);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] private static extern bool PostMessageW(IntPtr window, uint msg, IntPtr w, IntPtr l);
    public static void PressInWindow(IntPtr window, int key) {
        PostMessageW(window, 0x0100, new IntPtr(key), new IntPtr(1));
        PostMessageW(window, 0x0101, new IntPtr(key), new IntPtr(unchecked((int)0xC0000001)));
    }
    public static void Press(byte key, bool control) {
        if (control) keybd_event(0x11, 0, 0, UIntPtr.Zero);
        keybd_event(key, 0, 0, UIntPtr.Zero);
        keybd_event(key, 0, 2, UIntPtr.Zero);
        if (control) keybd_event(0x11, 0, 2, UIntPtr.Zero);
    }
    public static IntPtr Find(uint process, string title) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, unused) => {
            uint pid;
            GetWindowThreadProcessId(window, out pid);
            if (pid != process) return true;
            var text = new StringBuilder(512);
            GetWindowText(window, text, text.Capacity);
            if (text.ToString() != title) return true;
            result = window;
            return false;
        }, IntPtr.Zero);
        return result;
    }
}
'@

function Wait-Value([scriptblock]$Read, [string]$ErrorText) {
    $deadline = [Environment]::TickCount64 + 12000
    do {
        $value = & $Read
        if ($null -ne $value -and $value -ne $false) { return $value }
        Start-Sleep -Milliseconds 120
    } while ([Environment]::TickCount64 -lt $deadline)
    throw $ErrorText
}

function Named-Control($Window, [string]$Name) {
    $condition = [System.Windows.Automation.PropertyCondition]::new($uia::NameProperty, $Name)
    $Window.FindFirst($scope::Descendants, $condition)
}

function Window-For($Process, [string]$Title) {
    $window = [GpuiTestWindows]::Find($Process.Id, $Title)
    if ($window -ne [IntPtr]::Zero -and [GpuiTestWindows]::IsWindowVisible($window)) {
        $uia::FromHandle($window)
    }
}

function Invoke-Control($Element) {
    $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Set-Value($Element, [string]$Text) {
    $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Text)
}

function Read-Value($Element) {
    $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
}

function Select-Control($Element) {
    $Element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
}

function Save-Tree($Element, [string]$Path) {
    $nodes = $Element.FindAll($scope::Descendants, $everything)
    $lines = for ($i = 0; $i -lt $nodes.Count; $i++) {
        $item = $nodes.Item($i)
        $names = @($item.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
        '{0} | {1} | enabled={2} | {3}' -f $item.Current.ControlType.ProgrammaticName, $item.Current.Name, $item.Current.IsEnabled, ($names -join ', ')
    }
    $lines | Set-Content -Encoding utf8 $Path
    $nodes.Count
}

$exe = (Resolve-Path -LiteralPath $Executable).Path
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
[IO.Directory]::CreateDirectory($evidence) > $null
$private = Join-Path $evidence ('private-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($private) > $null
$start = [Diagnostics.ProcessStartInfo]::new($exe)
$start.UseShellExecute = $false
$start.Environment['TEMP'] = $private
$start.Environment['TMP'] = $private
$start.ArgumentList.Add('--comparison-review')
$start.ArgumentList.Add('--ready-file')
$start.ArgumentList.Add((Join-Path $evidence ('first-frame-' + [Guid]::NewGuid().ToString('N') + '.txt')))
$process = [Diagnostics.Process]::Start($start)
try {
    $main = Wait-Value {
        if ($process.HasExited) { throw "GPUI exited before UIA inspection with code $($process.ExitCode)." }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { $uia::FromHandle($process.MainWindowHandle) }
    } 'GPUI main window unavailable.'
    $search = Wait-Value { Named-Control $main 'Find an instance' } 'GPUI app controls are missing from UIA.'
    $mainCount = Save-Tree $main (Join-Path $evidence 'windows-main-tree.txt')
    if ($mainCount -lt 100) { throw "Expected the disposable library semantics in UIA, got $mainCount descendants." }
    if ($ExerciseKeyboardAccelerator) {
        $hwnd = [IntPtr]::new($main.Current.NativeWindowHandle)
        if ([GpuiTestWindows]::GetForegroundWindow() -ne $hwnd) { throw 'Keyboard regression needs the disposable launcher in the foreground.' }
        [GpuiTestWindows]::Press(0x46, $true)
        foreach ($digit in @(0x30, 0x39, 0x39)) {
            Start-Sleep -Milliseconds 60
            if ([GpuiTestWindows]::GetForegroundWindow() -ne $hwnd) { throw 'Disposable launcher lost foreground during keyboard input.' }
            [GpuiTestWindows]::Press($digit, $false)
        }
        Wait-Value { (Read-Value $search) -eq '099' } "Ctrl+F did not preserve the first search character." > $null
        Set-Value $search ''
        'PASS Ctrl+F followed by physical 099 reached the live search input.'
    }
    $search.SetFocus()
    Wait-Value { $search.Current.HasKeyboardFocus } 'UIA focus did not reach the search field.' > $null
    Set-Value $search '099'
    Wait-Value { (Read-Value $search) -eq '099' } 'UIA value update did not change the search input.' > $null
    $filtered = Wait-Value { Named-Control $main 'Review instance 099, Vanilla 24w14a' } 'Filtered profile could not be located.'
    if (Named-Control $main 'Review instance 098, Vanilla 1.20.4') { throw 'Search retained a nonmatching result.' }
    Select-Control $filtered
    Wait-Value { Named-Control $main 'Review instance 099' } 'UIA selection did not update the instance detail.' > $null
    $selected = $filtered.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected
    if (!$selected) { throw 'The selected profile state was not exposed.' }
    $new = Wait-Value {
        $action = Named-Control $main 'New instance'
        if (!$action) { $action = Named-Control $main '+  New instance' }
        $action
    } 'Missing New instance action.'
    Invoke-Control $new
    $modal = Wait-Value { Window-For $process 'New vanilla instance' } 'UIA did not open the actual create window.'
    $name = Wait-Value { Named-Control $modal 'Instance name' } 'Missing name edit in the dialog.'
    $modalCount = Save-Tree $modal (Join-Path $evidence 'windows-dialog-tree.txt')
    if ($main.Current.NativeWindowHandle -eq $modal.Current.NativeWindowHandle) { throw 'Expected a separate dialog HWND.' }
    foreach ($field in @('Maximum memory (MiB)', 'Window width', 'Window height')) {
        $element = Named-Control $modal $field
        if (!$element -or !(Read-Value $element)) { throw "Missing editable dialog field $field." }
    }
    $version = Named-Control $modal '26.3'
    $version.SetFocus()
    Wait-Value { $version.Current.HasKeyboardFocus } 'UIA focus did not reach the editor version choice.' > $null
    $save = Named-Control $modal 'Create instance'
    Invoke-Control $save
    Wait-Value {
        $nodes = $modal.FindAll($scope::Descendants, $everything)
        for ($i = 0; $i -lt $nodes.Count; $i++) {
            if ($nodes.Item($i).Current.Name -like 'Could not save this instance*') { $true; break }
        }
    } 'A rejected Create did not expose validation.' > $null
    $name.SetFocus()
    Wait-Value { $name.Current.HasKeyboardFocus } 'UIA focus did not reach the instance name.' > $null
    Set-Value $name 'AccessKit verification fixture'
    Wait-Value { (Read-Value $name) -eq 'AccessKit verification fixture' } 'UIA edit did not reach the GPUI form.' > $null
    Invoke-Control $save
    Wait-Value { $null -eq (Window-For $process 'New vanilla instance') } 'The valid form did not close after saving.' > $null
    Wait-Value { Named-Control $main 'AccessKit verification fixture' } 'The saved profile was not selected.' > $null
    $profiles = Get-ChildItem -LiteralPath $private -Recurse -File -Filter profiles.json | Select-Object -First 1
    $saved = (Get-Content -LiteralPath $profiles.FullName -Raw | ConvertFrom-Json).profiles |
        Where-Object name -eq 'AccessKit verification fixture'
    if (@($saved).Count -ne 1) { throw 'No unique profile was persisted by the actual app action.' }
    Select-Control (Wait-Value { Named-Control $main 'Settings' } 'Missing instance settings.')
    function Open-IconPicker {
        Invoke-Control (Wait-Value {
            $button = Named-Control $main 'Change icon'
            if ($button -and $button.Current.IsEnabled) { $button }
        } 'Missing Change icon action.')
        Wait-Value { Window-For $process 'Choose instance icon' } 'Missing separate icon window.'
    }
    $picker = Open-IconPicker
    $hwnd = [IntPtr]::new($picker.Current.NativeWindowHandle)
    Wait-Value { (Named-Control $picker 'Meadow block').Current.HasKeyboardFocus } 'Default sprite did not receive focus.' > $null
    [GpuiTestWindows]::PressInWindow($hwnd, 0x27)
    Wait-Value { (Named-Control $picker 'Red fox').Current.HasKeyboardFocus -and (Named-Control $picker 'Preview: Red fox') } 'Right did not select and focus the next sprite.' > $null
    $cancel = Named-Control $picker 'Cancel'
    $cancel.SetFocus()
    Wait-Value { $cancel.Current.HasKeyboardFocus } 'Cancel did not receive focus.' > $null
    [GpuiTestWindows]::PressInWindow($hwnd, 0x23)
    [GpuiTestWindows]::PressInWindow($hwnd, 0x27)
    # Allow both queued key events to repaint before inspecting the selected radio.
    Start-Sleep -Milliseconds 180
    if (!$cancel.Current.HasKeyboardFocus -or !(Named-Control $picker 'Preview: Red fox')) {
        throw 'A navigation key on the footer changed the selected sprite or focus.'
    }

    # Emulate another writer changing only this disposable instance while its icon picker is open.
    $index = Get-Content -LiteralPath $profiles.FullName -Raw | ConvertFrom-Json
    $row = $index.profiles | Where-Object id -eq $saved.id
    $row | Add-Member -NotePropertyName iconId -NotePropertyValue 'pine'
    [IO.File]::WriteAllText($profiles.FullName, ($index | ConvertTo-Json -Depth 10 -Compress))
    Invoke-Control (Named-Control $picker 'Save icon')
    Wait-Value { $picker.FindAll($scope::Descendants, $everything) | Where-Object { $_.Current.Name -like '*icon changed in another window*' } | Select-Object -First 1 } 'A stale icon write was not rejected.' > $null
    Invoke-Control $cancel
    Wait-Value { $null -eq (Window-For $process 'Choose instance icon') } 'Cancel did not close the icon window.' > $null
    $picker = Open-IconPicker
    Wait-Value { (Named-Control $picker 'Pine').Current.HasKeyboardFocus -and (Named-Control $picker 'Preview: Pine') } 'Reopening did not load the other writer''s saved choice.' > $null
    Invoke-Control (Named-Control $picker 'Use default')
    Invoke-Control (Wait-Value { $saveIcon = Named-Control $picker 'Save icon'; if ($saveIcon.Current.IsEnabled) { $saveIcon } } 'Icon reset did not enable Save.')
    Wait-Value { $null -eq (Window-For $process 'Choose instance icon') } 'Saving the icon did not close its window.' > $null
    $reset = (Get-Content -LiteralPath $profiles.FullName -Raw | ConvertFrom-Json).profiles | Where-Object id -eq $saved.id
    if ($reset.PSObject.Properties['iconId']) { throw 'Use default did not remove the stored override.' }
    'PASS Windows icon picker keyboard focus, footer isolation, stale-edit recovery and persisted reset.'
    "PASS Windows UIA main descendants=$mainCount dialog descendants=$modalCount. Search value, focus, filter, selection, dialog, validation and persisted profile verified."
} finally {
    if (!$process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
}
