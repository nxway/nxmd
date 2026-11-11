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
    if (!$process.CloseMainWindow()) { throw 'Native window did not accept a close request' }
    if (!$process.WaitForExit(15000)) { throw 'Native application did not exit after window close' }
    if ($process.ExitCode -ne 0) { throw "Native application exited with code $($process.ExitCode)" }
    if ((Get-FileHash $note -Algorithm SHA256).Hash -ne $before) { throw 'Closing an unedited note changed its bytes' }
    'PASS: native window opened, responded, preserved an unedited UTF-8/CRLF note, and closed successfully.' | Tee-Object (Join-Path $resultDir 'result.txt')
} catch {
    "FAIL: $($_.Exception.Message)" | Tee-Object (Join-Path $resultDir 'result.txt')
    throw
} finally {
    $env:LOCALAPPDATA = $originalLocalAppData
    if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
    Remove-Item -Recurse -Force $root
}
