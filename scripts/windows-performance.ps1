param([string]$Baseline,[string]$Candidate)
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
$vault=New-Item -ItemType Directory -Force (Join-Path $root 'vault')
1..250 | ForEach-Object { [IO.File]::WriteAllText((Join-Path $vault "Note $_.md"),("# Note $_`n`nA **small** Markdown document.`n"*100)) }
$previous=$env:LOCALAPPDATA
$results=@()
try {
  foreach ($iteration in 1..3) {
    $order=if($iteration%2){@('baseline','candidate')}else{@('candidate','baseline')}
    foreach($variant in $order) {
      $exe=(Resolve-Path $(if($variant -eq 'baseline'){$Baseline}else{$Candidate})).Path
      $config=New-Item -ItemType Directory -Force (Join-Path $root "$variant-$iteration/nxmd")
      @{vault=$vault.FullName;tabs=@('Note 1.md');active=0} | ConvertTo-Json | Set-Content (Join-Path $config 'session.json') -Encoding utf8NoBOM
      $env:LOCALAPPDATA=Split-Path $config.FullName
      $watch=[Diagnostics.Stopwatch]::StartNew()
      $p=Start-Process $exe -PassThru
      try {
        do {Start-Sleep -Milliseconds 25;$p.Refresh();if($p.HasExited){throw 'Benchmark app exited'}} until($p.MainWindowHandle -ne 0 -or $watch.Elapsed.TotalSeconds -gt 30)
        if($p.MainWindowHandle -eq 0){throw 'No benchmark window'}
        $startup=$watch.Elapsed.TotalMilliseconds
        Start-Sleep -Seconds 2
        $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds
        $sample=[Diagnostics.Stopwatch]::StartNew();Start-Sleep -Seconds 5
        $p.Refresh()
        $results += [pscustomobject]@{variant=$variant;iteration=$iteration;windowAppearanceMs=[Math]::Round($startup,1);workingSetBytes=$p.WorkingSet64;privateBytes=$p.PrivateMemorySize64;idleCpuMs=[Math]::Round($p.TotalProcessorTime.TotalMilliseconds-$cpu,1);sampleMs=[Math]::Round($sample.Elapsed.TotalMilliseconds,1)}
        if(!$p.CloseMainWindow() -or !$p.WaitForExit(15000)){throw 'Benchmark app failed to close'}
      } finally {if(!$p.HasExited){Stop-Process -Id $p.Id -Force}}
    }
  }
  $results | ConvertTo-Json | Set-Content smoke-results/performance.json
  $results | Format-Table | Out-String | Write-Host
} finally {$env:LOCALAPPDATA=$previous;Remove-Item -Recurse -Force $root}
