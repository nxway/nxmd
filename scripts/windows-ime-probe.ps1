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
[void][NativeSmoke]::SetForegroundWindow($mainWindow)
Wait-SmokeForeground $mainWindow
[uint32]$owner = 0
$tid = [NativeSmoke]::GetWindowThreadProcessId($mainWindow,[ref]$owner)
$ime = [ImeProbe]::ImmGetDefaultIMEWnd($mainWindow)
$layouts = @(Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Keyboard Layouts' |
    Where-Object { $_.PSChildName -match '0411$' -and $_.GetValue('Ime File') } | ForEach-Object PSChildName)
$record.japaneseLayouts = $layouts
foreach ($layout in @($layouts) + @('00000411')) {
    $hkl = [ImeProbe]::LoadKeyboardLayout($layout,0)
    if ($hkl -eq [IntPtr]::Zero) { continue }
    [void][ImeProbe]::SendMessage($mainWindow,0x50,[IntPtr]::Zero,$hkl)
    if (([ImeProbe]::GetKeyboardLayout($tid).ToInt64() -band 0xffff) -eq 0x411) { break }
}
$record.keyboardLayout = [ImeProbe]::GetKeyboardLayout($tid).ToInt64()
if (($record.keyboardLayout -band 0xffff) -ne 0x411 -or $ime -eq [IntPtr]::Zero) { Record-Ime; return }
# IMC_SETOPENSTATUS, IMC_SETCONVERSIONMODE (native, full-width, Roman input).
[void][ImeProbe]::SendMessage($ime,0x283,[IntPtr]6,[IntPtr]1)
[void][ImeProbe]::SendMessage($ime,0x283,[IntPtr]2,[IntPtr]25)
$record.openStatus = [ImeProbe]::SendMessage($ime,0x283,[IntPtr]5,[IntPtr]::Zero).ToInt64()
if ($record.openStatus -eq 0) { Record-Ime; return }
[System.Windows.Forms.SendKeys]::SendWait('^a')
[ImeProbe]::Key(0x4b)
Start-Sleep -Milliseconds 150
[ImeProbe]::Key(0x41)
Start-Sleep -Milliseconds 1800
$record.paused = Read-ImeEvidence
$frame = Capture-Images 'ime-before-commit.png'; $frame.Dispose()
[ImeProbe]::Key(0x1b)
Start-Sleep -Milliseconds 1800
$record.cancelled = Read-ImeEvidence
# Select again, compose ka, then explicitly commit and undo.
[System.Windows.Forms.SendKeys]::SendWait('^a')
[ImeProbe]::Key(0x4b)
[ImeProbe]::Key(0x41)
Start-Sleep -Milliseconds 250
[ImeProbe]::Key(0x0d)
Start-Sleep -Milliseconds 1800
$record.committed = Read-ImeEvidence
if ($record.committed.disk -cne 'か') {
    $record.reason = 'Physical keys did not produce a verified Japanese composition'
    Record-Ime
    return
}
[System.Windows.Forms.SendKeys]::SendWait('^z')
Start-Sleep -Milliseconds 1800
$record.undone = Read-ImeEvidence
[System.Windows.Forms.SendKeys]::SendWait('^a')
[ImeProbe]::Key(0x4b)
[ImeProbe]::Key(0x41)
Start-Sleep -Milliseconds 1800
$record.beforeClose = Read-ImeEvidence
[void]$process.CloseMainWindow()
if (!$process.WaitForExit(10000)) { throw 'Window did not close during composition' }
$record.afterClose = Read-ImeEvidence
$record.status = 'OBSERVED'
$record.reason = 'Japanese physical-key composition and explicit commit observed'
Record-Ime
