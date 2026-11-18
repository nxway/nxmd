param([string]$Baseline,[string]$Candidate)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PerformanceWindow {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
'@
$root=Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
$vault=New-Item -ItemType Directory -Force (Join-Path $root 'vault')
1..250 | ForEach-Object { [IO.File]::WriteAllText((Join-Path $vault "Note $_.md"),("# Note $_`n`nA **small** Markdown document.`n"*100)) }
$short=[IO.File]::ReadAllText((Join-Path $vault 'Note 1.md'))
$block=@'
## Formatted section

7. A list item with **bold** and *italic* text, followed by a sentence long enough to wrap in narrower windows.
8. Second item
   4. Nested item
   5. Another nested item

| Name | Details | Right |
| :--- | :---: | ---: |
| **Bold** | A long sentence with ordinary words to wrap naturally in a table cell. | Café |
| *Italic* | [Link](https://example.com) and `code` | 日本語 🙂 |

***Combined*** and **bold with *nested italic* inside**.

'@
$mixed=($block+"`n")*120 + "`nEnd."
$previous=$env:LOCALAPPDATA
$results=@()
try {
  foreach($workload in @('short','formatted')) {
    $source=if($workload -eq 'short'){$short}else{$mixed}
    foreach ($iteration in 1..3) {
      $order=if($iteration%2){@('baseline','candidate')}else{@('candidate','baseline')}
      foreach($variant in $order) {
        [IO.File]::WriteAllText((Join-Path $vault 'Note 1.md'),$source,[Text.UTF8Encoding]::new($false))
        $exe=(Resolve-Path $(if($variant -eq 'baseline'){$Baseline}else{$Candidate})).Path
        $config=New-Item -ItemType Directory -Force (Join-Path $root "$workload-$variant-$iteration/nxmd")
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
          $working=$p.WorkingSet64;$private=$p.PrivateMemorySize64
          $idle=$p.TotalProcessorTime.TotalMilliseconds-$cpu;$sampleMs=$sample.Elapsed.TotalMilliseconds
          [void][PerformanceWindow]::SetForegroundWindow($p.MainWindowHandle)
          $latencies=@()
          foreach($key in @('a','b','c','d','e')) {
            [System.Windows.Forms.Clipboard]::SetText([Guid]::NewGuid().ToString())
            $ack=[Diagnostics.Stopwatch]::StartNew()
            [System.Windows.Forms.SendKeys]::SendWait('^{END}'+$key+'+{LEFT}^c')
            do {
              $p.Refresh();if($p.HasExited){throw 'Benchmark exited during editing'}
              if([System.Windows.Forms.Clipboard]::GetText() -ceq $key){break}
              Start-Sleep -Milliseconds 5
            } while($ack.Elapsed.TotalSeconds -lt 15)
            if([System.Windows.Forms.Clipboard]::GetText() -cne $key){throw 'Benchmark input/copy acknowledgement timed out'}
            $latencies += [Math]::Round($ack.Elapsed.TotalMilliseconds,1)
          }
          $results += [pscustomobject]@{variant=$variant;workload=$workload;sourceBytes=[Text.Encoding]::UTF8.GetByteCount($source);iteration=$iteration;windowAppearanceMs=[Math]::Round($startup,1);workingSetBytes=$working;privateBytes=$private;idleCpuMs=[Math]::Round($idle,1);sampleMs=[Math]::Round($sampleMs,1);inputCopyRoundTripMs=$latencies}
          if(!$p.CloseMainWindow() -or !$p.WaitForExit(15000)){throw 'Benchmark app failed to close'}
        } finally {if(!$p.HasExited){Stop-Process -Id $p.Id -Force}}
      }
    }
  }
  $results | ConvertTo-Json -Depth 5 | Set-Content smoke-results/performance.json
  $results | Format-Table | Out-String | Write-Host
} finally {$env:LOCALAPPDATA=$previous;Remove-Item -Recurse -Force $root}
