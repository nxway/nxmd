param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path $Executable).Path
$resultDir = New-Item -ItemType Directory -Force smoke-results
$root = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
$vault = New-Item -ItemType Directory -Path (Join-Path $root 'vault') -Force
$config = New-Item -ItemType Directory -Path (Join-Path $root 'localappdata/nxmd') -Force
$note = Join-Path $vault 'Smoke.md'
$text = "# Windows smoke test`r`n`r`n**Native** Markdown with 日本語.`r`n"
[IO.File]::WriteAllText($note, $text, [Text.UTF8Encoding]::new($false))
$before = (Get-FileHash $note -Algorithm SHA256).Hash
@{vault=$vault.FullName; tabs=@('Smoke.md'); active=0; recent=@()} | ConvertTo-Json | Set-Content (Join-Path $config 'session.json') -Encoding utf8NoBOM
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeSmoke {
    public delegate bool EnumProc(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    public static IntPtr Dialog(int pid) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((h,p)=> { uint id; GetWindowThreadProcessId(h,out id); var c=new StringBuilder(256); GetClassName(h,c,256);
            if(id==pid && IsWindowVisible(h) && c.ToString()=="#32770") {found=h; return false;} return true; },IntPtr.Zero);
        return found;
    }
    public static void Click(IntPtr h,int x,int y) { var p=new IntPtr((y<<16)|x); SendMessage(h,0x200,IntPtr.Zero,p); SendMessage(h,0x201,new IntPtr(1),p); SendMessage(h,0x202,IntPtr.Zero,p); }
}
'@
function Assert-Alive {
    $process.Refresh()
    if ($process.HasExited) { throw "App crashed during dialog test: $($process.ExitCode)" }
}
function Wait-Dialog {
    $until=[DateTime]::UtcNow.AddSeconds(20)
    do { Assert-Alive; $h=[NativeSmoke]::Dialog($process.Id); if ($h -ne [IntPtr]::Zero) { return $h }; Start-Sleep -Milliseconds 100 } until ([DateTime]::UtcNow -gt $until)
    throw 'Expected native dialog did not appear'
}
function Wait-DialogClosed {
    $until=[DateTime]::UtcNow.AddSeconds(20)
    do { Assert-Alive; if ([NativeSmoke]::Dialog($process.Id) -eq [IntPtr]::Zero -and [NativeSmoke]::IsWindowEnabled($mainWindow)) { Start-Sleep -Milliseconds 300; return }; Start-Sleep -Milliseconds 100 } until ([DateTime]::UtcNow -gt $until)
    throw 'Dialog did not close or left its owner disabled'
}
function Open-Dialog([string]$keys) {
    [void][NativeSmoke]::SetForegroundWindow($mainWindow)
    [System.Windows.Forms.SendKeys]::SendWait($keys)
    $h=Wait-Dialog
    [void][NativeSmoke]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 300
}
function Cancel-Dialog {
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    Wait-DialogClosed
}
function Select-Path([string]$path) {
    [System.Windows.Forms.SendKeys]::SendWait('%n')
    [System.Windows.Forms.SendKeys]::SendWait('^a')
    [System.Windows.Forms.SendKeys]::SendWait($path)
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Wait-DialogClosed
}
$originalLocalAppData = $env:LOCALAPPDATA
$process = $null
try {
    $env:LOCALAPPDATA = Split-Path $config.FullName
    $process = Start-Process -FilePath $exe -ArgumentList "`"$($vault.FullName)`"" -PassThru -RedirectStandardOutput (Join-Path $resultDir 'stdout.txt') -RedirectStandardError (Join-Path $resultDir 'stderr.txt')
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        Start-Sleep -Milliseconds 500
        $process.Refresh()
        if ($process.HasExited) { throw "Native app exited before opening a window; exit code $($process.ExitCode)" }
    } until ($process.MainWindowHandle -ne 0 -or [DateTime]::UtcNow -gt $deadline)
    if ($process.MainWindowHandle -eq 0) { throw 'No native top-level window appeared within 45 seconds' }
    if ($process.MainWindowTitle -ne 'nxmd') { throw "Unexpected window title: $($process.MainWindowTitle)" }
    Start-Sleep -Seconds 3
    $process.Refresh()
    if ($process.HasExited) { throw 'Native application terminated during startup smoke test' }
    if (!$process.Responding) { throw 'Native window is not responding to Windows messages' }
    if ((Get-FileHash $note -Algorithm SHA256).Hash -ne $before) { throw 'Opening a note changed its bytes' }
    $mainWindow=$process.MainWindowHandle
    # Exercise real native dialogs from GPUI event handlers, including cancel and accept.
    Open-Dialog '^o'; Cancel-Dialog
    Open-Dialog '^o'; Select-Path $vault.FullName
    Open-Dialog '^n'; Cancel-Dialog
    Open-Dialog '^n'; Select-Path (Join-Path $vault 'Created.md')
    if (!(Test-Path (Join-Path $vault 'Created.md'))) { throw 'New note dialog did not create the selected file' }
    # Existing sidebar commands: New folder, Save as copy, Rename/Move, Trash.
    foreach ($y in @(146,183,220,257)) {
        [void][NativeSmoke]::SetForegroundWindow($mainWindow)
        [NativeSmoke]::Click($mainWindow,80,$y)
        $dialog=Wait-Dialog
        [void][NativeSmoke]::SetForegroundWindow($dialog)
        Cancel-Dialog
    }
    if ((Get-FileHash $note -Algorithm SHA256).Hash -ne $before) { throw 'Dialog operations changed the existing note' }
    # Confirm completion callbacks ran, not merely that Windows closed the dialogs.
    $diagnostics=Get-ChildItem (Join-Path $config 'logs') -Filter '*.log' | Get-Content | Out-String
    foreach ($operation in @('open-vault','Note','Folder','Copy','Move','trash')) {
        if (!$diagnostics.Contains("dialog $operation completed")) { throw "Missing dialog completion: $operation" }
    }
    if (!$process.CloseMainWindow()) { throw 'Native window did not accept a close request' }
    if (!$process.WaitForExit(15000)) { throw 'Native application did not exit after window close' }
    if ($process.ExitCode -ne 0) { throw "Native application exited with code $($process.ExitCode)" }
    if ((Get-FileHash $note -Algorithm SHA256).Hash -ne $before) { throw 'Closing an unedited note changed its bytes' }
    $probe=Start-Process -FilePath $exe -ArgumentList '--test-panic-log' -PassThru
    if (!$probe.WaitForExit(15000)) { Stop-Process -Id $probe.Id -Force; throw 'Panic logging probe hung' }
    if ($probe.ExitCode -eq 0) { throw 'Panic probe unexpectedly succeeded' }
    $panicLogs=Get-ChildItem (Join-Path $config 'logs') -Filter '*.log' | Get-Content | Out-String
    if (!$panicLogs.Contains('nxmd diagnostic self-test') -or !$panicLogs.Contains('PANIC:')) { throw 'Persistent panic log was not written' }
    'PASS: native dialogs opened/cancelled; Open vault and New note accepted; all six dialog callbacks completed; note preserved; clean shutdown; persistent panic log verified.'  | Tee-Object (Join-Path $resultDir 'result.txt')
} catch {
    "FAIL: $($_.Exception.Message)" | Tee-Object (Join-Path $resultDir 'result.txt')
    throw
} finally {
    $env:LOCALAPPDATA = $originalLocalAppData
    if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
    if (Test-Path (Join-Path $config 'logs')) { Copy-Item (Join-Path $config 'logs') (Join-Path $resultDir 'logs') -Recurse -Force }
    Remove-Item -Recurse -Force $root
}
