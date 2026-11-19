# Dot-sourced after the original smoke app has exited. Each launch is a real,
# fresh nxmd process sharing only this test profile and this temporary vault.
$config = New-Item -ItemType Directory -Force (Join-Path $root 'recovery-profile/nxmd')
$env:LOCALAPPDATA = Split-Path $config.FullName
$testVault = New-Item -ItemType Directory -Force (Join-Path $root 'recovery-vault')
$source = Join-Path $testVault 'Recovery.md'
$separate = Join-Path $testVault 'Separate.md'
$disk = 'External Привет'
$base = 'Before crash'
$mine = $base + 'x'
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($source,$base,$utf8)
@{vault=$testVault.FullName; tabs=@('Recovery.md'); active=0} | ConvertTo-Json | Set-Content (Join-Path $config 'session.json') -Encoding utf8NoBOM
function Start-RecoveryApp {
    $script:process = Start-Process -FilePath $exe -ArgumentList "`"$($testVault.FullName)`"" -PassThru
    Wait-ImageCondition { $process.Refresh(); $process.MainWindowTitle -ceq 'nxmd' } 'Recovery workspace did not launch'
    $script:mainWindow = $process.MainWindowHandle
    [void][NativeSmoke]::SetForegroundWindow($mainWindow)
    Wait-ImageCondition { [NativeSmoke]::GetForegroundWindow() -eq $mainWindow } 'Recovery workspace has no foreground focus'
}
function Stop-RecoveryApp {
    Stop-Process -Id $process.Id -Force
    if (!$process.WaitForExit(10000)) { throw 'Forced termination did not complete' }
}
function Read-Draft {
    $files = @(Get-ChildItem (Join-Path $config 'recovery-v1') -Filter draft.json -Recurse -ErrorAction SilentlyContinue)
    if ($files.Count -eq 1) {
        $snapshot=Read-SmokeSnapshot $files[0].FullName
        if ($null -ne $snapshot) { return ($snapshot | ConvertFrom-Json).draft }
    }
}
function Open-Recovery([string]$expected) {
    Start-RecoveryApp
    [System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
    Assert-EditorText $disk
    [System.Windows.Forms.SendKeys]::SendWait('^+r')
    Wait-ImageCondition {
        $h = [NativeSmoke]::GetForegroundWindow()
        $label = [Text.StringBuilder]::new(256)
        [uint32]$owner = 0
        [void][NativeSmoke]::GetWindowThreadProcessId($h,[ref]$owner)
        [void][NativeSmoke]::GetWindowText($h,$label,256)
        $owner -eq $process.Id -and $label.ToString().EndsWith('Recovered draft')
    } 'Native recovery review did not open'
    $script:mainWindow = [NativeSmoke]::GetForegroundWindow()
    Assert-EditorText $expected
}
Start-RecoveryApp
[System.Windows.Forms.Clipboard]::SetText([guid]::NewGuid().ToString())
Assert-EditorText $base
[IO.File]::WriteAllText($source,$disk,$utf8)
$diskHash = (Get-FileHash $source).Hash
[System.Windows.Forms.SendKeys]::SendWait('x')
Wait-ImageCondition { (Read-Draft).text -ceq $mine } 'Dirty conflicting edits never reached recovery storage'
$generation = (Read-Draft).generation
[System.Windows.Forms.SendKeys]::SendWait('^w')
Wait-ImageCondition { (Image-Logs).Contains('Save blocked') } 'Closing a conflicting tab did not report its failed save'
if (!$process.CloseMainWindow()) { throw 'Could not send close request' }
Wait-ImageCondition { (Image-Logs).Contains('Window close blocked') } 'Unsaved conflict did not protect application exit'
if ((Get-FileHash $source).Hash -ne $diskHash) { throw 'Conflict or exit overwrote external bytes' }
Stop-RecoveryApp
Open-Recovery $mine
Stop-RecoveryApp
if ((Read-Draft).generation -ne $generation) { throw 'Opening a recovery draft consumed or changed its generation' }
Open-Recovery $mine
[System.Windows.Forms.SendKeys]::SendWait('y')
$latest = $mine + 'y'
Wait-ImageCondition { $d=Read-Draft; $d.text -ceq $latest -and $d.generation -gt $generation } 'New edits to a recovered draft were not checkpointed'
Stop-RecoveryApp
Open-Recovery $latest
Open-Dialog '^s'
[System.Windows.Forms.SendKeys]::SendWait('%n')
[System.Windows.Forms.SendKeys]::SendWait('^a')
[System.Windows.Forms.SendKeys]::SendWait($separate)
Invoke-DialogButton 'Save'
Wait-SavedText $separate $latest 'Recovered copy did not contain the latest generation'
Wait-ImageCondition { @(Get-ChildItem (Join-Path $config 'recovery-v1') -Filter draft.json -Recurse).Count -eq 0 } 'Saved generation was not retired'
if ((Get-FileHash $source).Hash -ne $diskHash) { throw 'Recovery changed the external original' }
$process.Refresh()
if (!$process.CloseMainWindow() -or !$process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Recovery workspace failed to close cleanly' }
'PASS: conflicted close and exit; three forced terminations; two draft generations; fresh-process restore and independent copy; exact original bytes preserved.' | Tee-Object (Join-Path $resultDir 'recovery-result.txt')
