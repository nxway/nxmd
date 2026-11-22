# Dot-sourced in the isolated vault and foreground window from windows-smoke.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class TableViewport {
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int height,bool paint);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
}
'@
function Save-TableFrame([string]$Name) {
    Start-Sleep -Milliseconds 450
    Assert-Alive
    $frame = Capture-Images "$Name.png"
    $frame.Dispose()
}
function Send-TableWheel([int]$Message, [int]$Delta) {
    $pos = [NativeSmoke]::ScreenPoint($mainWindow,450,210)
    [void][TableViewport]::SetCursorPos($pos[0],$pos[1])
    [void][NativeSmoke]::SendMessage($mainWindow,$Message,[IntPtr]($Delta -shl 16),[IntPtr](($pos[1] -shl 16) -bor $pos[0]))
}
# The window is at the desktop origin. Locate the actual first column divider
# inside Day A's first body row, using its continuous border between glyphs.
function Get-ExerciseDivider($Frame) {
    $rails = @(for ($x=400; $x -lt 710; $x++) {
        $votes = 0
        foreach ($y in 285..305) {
            $c = $Frame.GetPixel($x,$y)
            if ($c.R -eq 87 -and $c.G -eq 97 -and $c.B -eq 90) { $votes++ }
        }
        if ($votes -ge 18) { $x }
    })
    if ($rails.Count -lt 2) { throw 'Missing first exercise cell borders' }
    return $rails[0]
}
Wait-SmokeForeground $mainWindow
[void][TableViewport]::SetCursorPos(15,15)
if ($TableFixture -eq 'exercise') {
    foreach ($width in 720,750,780,850,1000) {
        if (![TableViewport]::MoveWindow($mainWindow,0,0,$width,730,$true)) { throw 'Cannot resize the native window' }
        [System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
        Send-TableWheel 0x20a 2400
        [void][TableViewport]::SetCursorPos(15,15)
        Save-TableFrame "day-a-$width"
        Send-TableWheel 0x20a -3600
        [void][TableViewport]::SetCursorPos(15,15)
        Save-TableFrame "day-b-$width"
    }
    # Enter the final Notes cell, paste a sentence that reflows the row,
    # and prove the native caret inserted it inside the intended source cell.
    [System.Windows.Forms.SendKeys]::SendWait('^{END}')
    [System.Windows.Forms.SendKeys]::SendWait('{LEFT 8}')
    $marker = ' Maintain control throughout the entire movement. '
    [System.Windows.Forms.Clipboard]::SetText($marker)
    [System.Windows.Forms.SendKeys]::SendWait('^v')
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    $edited = Wait-SavedInsertion $note $marker
    $at = $edited.IndexOf($marker)
    $cell = 'Pull back and hold.'
    $start = $text.IndexOf($cell)
    if ($start -lt 0 -or $at -lt $start -or $at -gt $start+$cell.Length -or
        $edited.Remove($at,$marker.Length) -cne $text) { throw 'Native edit missed the final Notes cell or changed surrounding Markdown' }
    [void][TableViewport]::SetCursorPos(15,15)
    Save-TableFrame 'day-b-edited-1000'
    [System.Windows.Forms.SendKeys]::SendWait('^z')
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    Wait-SavedText $note $text 'Undo after table reflow'
    if ((Read-SmokeSnapshot $note -Hash) -ne $before) { throw 'Automatic reflow changed Markdown bytes' }
    [System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
    Send-TableWheel 0x20a 2400
    [void][TableViewport]::SetCursorPos(15,15)
    Start-Sleep -Milliseconds 500
    $frame = Capture-Images 'before-manual-resize.png'
    try { $divider = Get-ExerciseDivider $frame } finally { $frame.Dispose() }
    [void][TableViewport]::SetCursorPos($divider,300)
    Start-Sleep -Milliseconds 300
    [TableViewport]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    try {
        foreach ($dx in 8,16,24,32,40) {
            [void][TableViewport]::SetCursorPos(($divider+$dx),300)
            Start-Sleep -Milliseconds 70
        }
    } finally { [TableViewport]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    $saved = Wait-SavedInsertion $note '<!-- table:grid cols='
    $pattern = '(?m)^<!-- table:grid cols=(?:[0-9]+,){4}[0-9]+ -->\r?\n'
    $markerLine = [regex]::Match($saved,$pattern)
    if (!$markerLine.Success -or $markerLine.Index -ne $text.IndexOf('| Exercise') -or
        $saved.Remove($markerLine.Index,$markerLine.Length) -cne $text) { throw 'Manual resize changed unrelated source' }
    [System.Windows.Forms.SendKeys]::SendWait('^z')
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    Wait-SavedText $note $text 'Undo manual column resize'
    [System.Windows.Forms.SendKeys]::SendWait('^+z')
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    Wait-SavedText $note $saved 'Redo manual column resize'
    $savedHash = Read-SmokeSnapshot $note -Hash
    [System.Windows.Forms.SendKeys]::SendWait('^w')
    Wait-ImageCondition { (Active-ImagePath) -eq '' } 'Resized note did not close'
    Open-EditorFixture 0 'Smoke.md'
    [System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
    [void][TableViewport]::SetCursorPos(15,15)
    Start-Sleep -Milliseconds 500
    $frame = Capture-Images 'reopened-manual-resize.png'
    try { $reopened = Get-ExerciseDivider $frame } finally { $frame.Dispose() }
    if ([Math]::Abs($reopened-$divider-40) -gt 2) { throw "Reopened column divider at $reopened, expected $($divider+40)" }
    if ((Read-SmokeSnapshot $note -Hash) -ne $savedHash) { throw 'Reopening changed persisted table bytes' }
    "Manual resize persisted across save, close and reopen: divider $divider -> $reopened; $($markerLine.Value.Trim())" | Add-Content (Join-Path $resultDir 'input-sync.txt')
} else {
    [void][TableViewport]::MoveWindow($mainWindow,0,0,780,730,$true)
    Save-TableFrame 'wide-left'
    for ($i=0; $i -lt 8; $i++) { Send-TableWheel 0x20e 120 }
    Save-TableFrame 'wide-right'
    for ($i=0; $i -lt 32; $i++) { Send-TableWheel 0x20e 120 }
    [void][TableViewport]::SetCursorPos(15,15)
    Save-TableFrame 'wide-far-edge'
    Send-TableWheel 0x20a -1200
    Save-TableFrame 'wide-scrolled'
    if ((Read-SmokeSnapshot $note -Hash) -ne $before) { throw 'Scrolling changed Markdown bytes' }
}
