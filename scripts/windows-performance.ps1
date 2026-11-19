param([string]$Baseline,[string]$Candidate)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PerformanceWindow {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
function Confirm-BenchmarkEditor($App,[string]$Source) {
    # Only retry non-mutating activation and copy. The measured keystrokes
    # below are sent exactly once, after the editor returns its source.
    $deadline=[Diagnostics.Stopwatch]::StartNew()
    $handle=$App.MainWindowHandle
    [System.Windows.Forms.Clipboard]::SetText([Guid]::NewGuid().ToString())
    do {
        $App.Refresh()
        if ($App.HasExited) { throw 'Benchmark app exited during input preflight' }
        if ([PerformanceWindow]::GetForegroundWindow() -ne $handle) {
            [void][PerformanceWindow]::SetForegroundWindow($handle)
            Start-Sleep -Milliseconds 100
            continue
        }
        [System.Windows.Forms.SendKeys]::SendWait('^a')
        [System.Windows.Forms.SendKeys]::SendWait('^c')
        $copied=[Diagnostics.Stopwatch]::StartNew()
        do {
            if ([System.Windows.Forms.Clipboard]::GetText() -ceq $Source) {
                [System.Windows.Forms.SendKeys]::SendWait('^{END}')
                return
            }
            Start-Sleep -Milliseconds 25
        } while ($copied.ElapsedMilliseconds -lt 500)
    } while ($deadline.Elapsed.TotalSeconds -lt 15)
    throw "Benchmark editor did not acknowledge its source. Foreground=$([PerformanceWindow]::GetForegroundWindow()), expected=$handle"
}
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
        Write-Host "Benchmark workload=$workload variant=$variant iteration=$iteration"
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
          Confirm-BenchmarkEditor $p $source
          $latencies=@()
          foreach($key in @('a','b','c','d','e')) {
            [System.Windows.Forms.Clipboard]::SetText([Guid]::NewGuid().ToString())
            $ack=[Diagnostics.Stopwatch]::StartNew()
            [System.Windows.Forms.SendKeys]::SendWait('^{END}')
            [System.Windows.Forms.SendKeys]::SendWait($key)
            [System.Windows.Forms.SendKeys]::SendWait('+{LEFT}')
            [System.Windows.Forms.SendKeys]::SendWait('^c')
            do {
              $p.Refresh();if($p.HasExited){throw 'Benchmark exited during editing'}
              if([System.Windows.Forms.Clipboard]::GetText() -ceq $key){break}
              Start-Sleep -Milliseconds 5
            } while($ack.Elapsed.TotalSeconds -lt 15)
            if([System.Windows.Forms.Clipboard]::GetText() -cne $key){throw "Benchmark input/copy timed out for $variant $workload iteration=$iteration key=$key foreground=$([PerformanceWindow]::GetForegroundWindow()) expectedWindow=$($p.MainWindowHandle) copiedLength=$([System.Windows.Forms.Clipboard]::GetText().Length)"}
            $latencies += [Math]::Round($ack.Elapsed.TotalMilliseconds,1)
          }
          $results += [pscustomobject]@{variant=$variant;workload=$workload;sourceBytes=[Text.Encoding]::UTF8.GetByteCount($source);iteration=$iteration;windowAppearanceMs=[Math]::Round($startup,1);workingSetBytes=$working;privateBytes=$private;idleCpuMs=[Math]::Round($idle,1);sampleMs=[Math]::Round($sampleMs,1);inputCopyRoundTripMs=$latencies}
          if(!$p.CloseMainWindow() -or !$p.WaitForExit(15000)){throw 'Benchmark app failed to close'}
          if ([IO.File]::ReadAllText((Join-Path $vault 'Note 1.md')) -cne ($source+'abcde')) {throw 'Benchmark input changed unexpected source bytes'}
        } finally {if(!$p.HasExited){Stop-Process -Id $p.Id -Force}}
      }
    }
  }
  $results | ConvertTo-Json -Depth 5 | Set-Content smoke-results/performance.json
  $results | Format-Table | Out-String | Write-Host
} finally {$env:LOCALAPPDATA=$previous;Remove-Item -Recurse -Force $root}
