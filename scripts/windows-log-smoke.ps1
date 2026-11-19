# Exercise retention with a real live owner, fresh processes and an isolated
# profile. Synthetic legacy archives use the historical nxmd header and name.
$logProfile=New-Item -ItemType Directory (Join-Path $root 'log-profile')
$priorProfile=$env:LOCALAPPDATA
$priorDetail=$env:NXMD_DIAGNOSTICS
$env:LOCALAPPDATA=$logProfile.FullName
$env:NXMD_DIAGNOSTICS=$null
$ownerProcess=$null
try {
    $logVault=New-Item -ItemType Directory (Join-Path $root 'log-vault')
    $ownerProcess=Start-Process $exe -ArgumentList "`"$($logVault.FullName)`"" -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(25)
    do {Start-Sleep -Milliseconds 100;$ownerProcess.Refresh()} until ($ownerProcess.MainWindowHandle -ne 0 -or $ownerProcess.HasExited -or [DateTime]::UtcNow -gt $deadline)
    if ($ownerProcess.MainWindowHandle -eq 0) {throw 'Log owner did not open its window'}
    $logDir=Join-Path $logProfile 'nxmd/logs'
    $ownerLog=@(Get-ChildItem $logDir -Filter "*-$($ownerProcess.Id).log")
    if ($ownerLog.Count -ne 1) {throw 'Missing active instance log'}
    $now=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $utf8=[Text.UTF8Encoding]::new($false)
    $stampHeader="SystemTime { fixture } INFO nxmd::diagnostics: nxmd old started`n"
    foreach ($n in 1..24) {
        [IO.File]::WriteAllText((Join-Path $logDir "nxmd-$($now-1000*$n)-2147483647.log"),$stampHeader,$utf8)
    }
    $old=Join-Path $logDir 'nxmd-1-2147483647.log'
    [IO.File]::WriteAllText($old,$stampHeader,$utf8)
    $unknown=Join-Path $logDir 'nxmd-2-2147483647.log'
    [IO.File]::WriteAllText($unknown,'foreign',$utf8)
    $recovery=New-Item -ItemType Directory -Force (Join-Path $logProfile 'nxmd/recovery-v1')
    $sentinel=Join-Path $recovery 'retention-sentinel'
    [IO.File]::WriteAllText($sentinel,'retain',$utf8)
    foreach ($n in 1..3) {
        $panic=Start-Process $exe -ArgumentList '--test-panic-log' -PassThru
        if (!$panic.WaitForExit(15000) -or $panic.ExitCode -eq 0) {throw 'Log panic probe did not terminate as expected'}
    }
    if ($ownerProcess.HasExited -or !(Test-Path $ownerLog[0].FullName)) {throw 'Pruning removed a live log'}
    if ((Get-ChildItem $logDir -Filter '*.log').Count -gt 11 -or (Test-Path $old)) {throw 'Retention failed to prune inactive logs'}
    if ([IO.File]::ReadAllText($unknown) -cne 'foreign' -or [IO.File]::ReadAllText($sentinel) -cne 'retain') {throw 'Retention altered unrelated data'}
    $panicText=Get-ChildItem $logDir -Filter '*.log' | Get-Content | Out-String
    if (!$panicText.Contains('PANIC: nxmd diagnostic self-test')) {throw 'Retention lost recent panic report'}
    if (!$ownerProcess.CloseMainWindow() -or !$ownerProcess.WaitForExit(15000)) {throw 'Diagnostic owner did not shut down cleanly'}
    'PASS: active owner survives concurrent launches, old logs pruned, panic preserved, unrelated log and recovery sentinel intact.' | Tee-Object (Join-Path $resultDir 'retention-result.txt')
} finally {
    if ($ownerProcess -and !$ownerProcess.HasExited) {$ownerProcess.Kill();$ownerProcess.WaitForExit()}
    $env:LOCALAPPDATA=$priorProfile
    $env:NXMD_DIAGNOSTICS=$priorDetail
}
