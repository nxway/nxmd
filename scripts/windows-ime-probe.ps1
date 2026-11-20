# Run in the foreground nxmd test window. Send physical letter keys through the
# installed Japanese IME. Record persistence independently of editor events.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ImeProbe {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string name, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint tid);
    [DllImport("imm32.dll")] public static extern IntPtr ImmGetDefaultIMEWnd(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk,byte scan,uint flags,UIntPtr extra);
    public static void Key(byte vk) {
        keybd_event(vk,0,0,UIntPtr.Zero);
        keybd_event(vk,0,2,UIntPtr.Zero);
    }
}
'@
$record = [ordered]@{ languages=@((Get-WinUserLanguageList).LanguageTag); status='BLOCKED'; reason='Japanese IME activation unverified' }
function Record-Ime {
    $record | ConvertTo-Json -Depth 5 | Tee-Object (Join-Path $resultDir 'ime-observation.json')
}
function Read-ImeEvidence {
    $drafts = @(Get-ChildItem (Join-Path $config 'recovery-v1') -Filter draft.json -Recurse -ErrorAction SilentlyContinue |
        ForEach-Object { (Get-Content $_ -Raw | ConvertFrom-Json).draft.text })
    @{ disk = [IO.File]::ReadAllText($note); drafts = $drafts }
}
function Enable-JapaneseInput {
    Wait-SmokeForeground $mainWindow
    [uint32]$owner = 0
    $tid = [NativeSmoke]::GetWindowThreadProcessId($mainWindow,[ref]$owner)
    $ime = [ImeProbe]::ImmGetDefaultIMEWnd($mainWindow)
    $layouts = @(Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Keyboard Layouts' |
        Where-Object { $_.PSChildName -match '0411$' -and $_.GetValue('Ime File') } | ForEach-Object PSChildName)
    foreach ($layout in @($layouts) + @('00000411')) {
        $hkl = [ImeProbe]::LoadKeyboardLayout($layout,0)
        if ($hkl -eq [IntPtr]::Zero) { continue }
        [void][ImeProbe]::SendMessage($mainWindow,0x50,[IntPtr]::Zero,$hkl)
        if (([ImeProbe]::GetKeyboardLayout($tid).ToInt64() -band 0xffff) -eq 0x411) { break }
    }
    $record.keyboardLayout = [ImeProbe]::GetKeyboardLayout($tid).ToInt64()
    if (($record.keyboardLayout -band 0xffff) -ne 0x411 -or $ime -eq [IntPtr]::Zero) {
        Record-Ime
        throw 'Japanese input profile unavailable on this runner'
    }
    [void][ImeProbe]::SendMessage($ime,0x283,[IntPtr]6,[IntPtr]1)
    [void][ImeProbe]::SendMessage($ime,0x283,[IntPtr]2,[IntPtr]25)
    $record.openStatus = [ImeProbe]::SendMessage($ime,0x283,[IntPtr]5,[IntPtr]::Zero).ToInt64()
}
function Assert-ImeSelection([string]$expected) {
    [System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
    [System.Windows.Forms.SendKeys]::SendWait('^c')
    Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $expected } 'IME selection differs from the original selection'
}
function Select-ImeFixture([string]$expected) {
    [System.Windows.Forms.SendKeys]::SendWait('^a')
    Assert-ImeSelection $expected
}
function Type-Ka {
    [ImeProbe]::Key(0x4b)
    Start-Sleep -Milliseconds 200
    [ImeProbe]::Key(0x41)
    Start-Sleep -Milliseconds 1800
}
function Restart-ImeApp {
    $script:process = Start-Process -FilePath $exe -ArgumentList "`"$($vault.FullName)`"" -PassThru
    Wait-ImageCondition { $process.Refresh(); $process.MainWindowTitle -ceq 'nxmd' } 'IME fixture did not reopen'
    $script:mainWindow = $process.MainWindowHandle
    Enable-JapaneseInput
}
function Check-ImeDisk([string]$expected,[string]$stage) {
    $snapshot = Read-ImeEvidence
    $record[$stage] = $snapshot
    Record-Ime
    if ($snapshot.disk -cne $expected -or $snapshot.drafts.Count -gt 0) {
        throw "IME persistence mismatch at $stage"
    }
}

Enable-JapaneseInput
Select-ImeFixture $text
Type-Ka
$record.paused = Read-ImeEvidence
$frame = Capture-Images 'ime-before-commit.png'; $frame.Dispose()
[ImeProbe]::Key(0x0d)
Start-Sleep -Milliseconds 1800
$record.committed = Read-ImeEvidence
Record-Ime
if ($record.committed.disk -cne 'か') {
    throw 'Physical keys and Enter did not commit exactly one Japanese result'
}
$record.status = 'UNRESOLVED'
$record.reason = 'Native composition verified; checking persistence, history and recovery'
Record-Ime
if ($record.paused.disk -cne $text -or $record.paused.drafts.Count -gt 0) {
    throw 'Preedit reached disk or recovery before Enter'
}
[System.Windows.Forms.SendKeys]::SendWait('^z')
Start-Sleep -Milliseconds 1800
Check-ImeDisk $text 'undone'
Assert-ImeSelection $text
Select-ImeFixture $text
Type-Ka
# Escape can dismiss the prediction popup first. The second Escape cancels the preedit.
[ImeProbe]::Key(0x1b)
Start-Sleep -Milliseconds 200
[ImeProbe]::Key(0x1b)
Start-Sleep -Milliseconds 1200
Check-ImeDisk $text 'cancelled'
Assert-ImeSelection $text
Select-ImeFixture $text
Type-Ka
Check-ImeDisk $text 'beforeClose'
if (!$process.CloseMainWindow() -or !$process.WaitForExit(10000)) { throw 'Window did not close during composition' }
Check-ImeDisk $text 'afterClose'
Restart-ImeApp
[System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
Assert-EditorText $text

# Keep a real pre-existing dirty generation alive by publishing a disk conflict.
$external = 'external 版本'
$dirty = $text+'!'
[IO.File]::WriteAllText($note,$external,[Text.UTF8Encoding]::new($false))
[System.Windows.Forms.Clipboard]::SetText('!')
[System.Windows.Forms.SendKeys]::SendWait('^v')
Wait-ImageCondition { (Read-ImeEvidence).drafts -ccontains $dirty } 'Committed dirty text did not checkpoint'
Select-ImeFixture $dirty
Type-Ka
$record.beforeCrash = Read-ImeEvidence
Record-Ime
if ($record.beforeCrash.disk -cne $external -or $record.beforeCrash.drafts.Count -ne 1 -or
    $record.beforeCrash.drafts[0] -cne $dirty) { throw 'Preedit polluted the previous dirty generation' }
Stop-Process -Id $process.Id -Force
if (!$process.WaitForExit(10000)) { throw 'Forced termination did not complete' }
Restart-ImeApp
[System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
Assert-EditorText $external
[System.Windows.Forms.SendKeys]::SendWait('^+r')
Wait-ImageCondition {
    $h = [NativeSmoke]::GetForegroundWindow()
    $label = [Text.StringBuilder]::new(256)
    [uint32]$owner = 0
    [void][NativeSmoke]::GetWindowThreadProcessId($h,[ref]$owner)
    [void][NativeSmoke]::GetWindowText($h,$label,256)
    $owner -eq $process.Id -and $label.ToString().EndsWith('Recovered draft')
} 'IME recovery review did not open'
$mainWindow = [NativeSmoke]::GetForegroundWindow()
[System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
Assert-EditorText $dirty
$record.recoveredEditor = $dirty
$record.afterRecovery = Read-ImeEvidence
$record.status = 'PASS'
$record.reason = 'Physical Japanese input, cancel, single commit, undo, clean close and crash recovery verified'
Record-Ime
