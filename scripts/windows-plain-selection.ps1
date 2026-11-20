# Run via windows-smoke.ps1 -SelectionEnding LF or CRLF. Work in physical
# screen pixels throughout. Use observed ink to place endpoints between glyphs.
Add-Type -ReferencedAssemblies @('System.Drawing.Common','System.Drawing.Primitives','System.Runtime.InteropServices','System.Threading.Thread','System.Collections') -TypeDefinition @'
using System;
using System.Drawing;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class PlainSelection {
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd,int x,int y,int w,int h,bool repaint);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
    public static void Drag(Point from, Point to) {
        SetCursorPos(from.X,from.Y);
        mouse_event(2,0,0,0,UIntPtr.Zero);
        try {
            for(int i=1;i<=24;i++) {
                SetCursorPos(from.X+(to.X-from.X)*i/24,from.Y+(to.Y-from.Y)*i/24);
                System.Threading.Thread.Sleep(16);
            }
        } finally { mouse_event(4,0,0,0,UIntPtr.Zero); }
    }
    public static Rectangle[] Ink(Bitmap bmp, Rectangle area) {
        var rows=new List<Rectangle>();
        Rectangle row=Rectangle.Empty;
        for(int y=area.Top;y<area.Bottom;y++) {
            int left=area.Right, right=-1;
            for(int x=area.Left;x<area.Right;x++) {
                Color c=bmp.GetPixel(x,y);
                if(c.R>125 && c.G>125 && c.B>125) { left=Math.Min(left,x);right=x; }
            }
            if(right>=left) {
                var ink=Rectangle.FromLTRB(left,y,right+1,y+1);
                row=row.IsEmpty ? ink : Rectangle.Union(row,ink);
            } else if(!row.IsEmpty) { rows.Add(row);row=Rectangle.Empty; }
        }
        if(!row.IsEmpty) rows.Add(row);
        return rows.ToArray();
    }
    public static Rectangle TintBounds(Bitmap bmp,Rectangle area) {
        Rectangle bounds=Rectangle.Empty;
        for(int y=area.Top;y<area.Bottom;y++)
            for(int x=area.Left;x<area.Right;x++) {
                Color c=bmp.GetPixel(x,y);
                if(Math.Abs(c.R-64)<=3 && Math.Abs(c.G-74)<=3 && Math.Abs(c.B-61)<=3) {
                    var pixel=new Rectangle(x,y,1,1);
                    bounds=bounds.IsEmpty ? pixel : Rectangle.Union(bounds,pixel);
                }
            }
        return bounds;
    }
}
'@
[void][NativeSmoke]::SetForegroundWindow($mainWindow)
Wait-SmokeForeground $mainWindow
[void][PlainSelection]::MoveWindow($mainWindow,0,0,850,650,$true)
Start-Sleep -Milliseconds 400
$origin=[NativeSmoke]::ScreenPoint($mainWindow,0,0)
$dpi=[PlainSelection]::GetDpiForWindow($mainWindow)
$scale=$dpi/96.0
$area=[Drawing.Rectangle]::new($origin[0]+[int](265*$scale),$origin[1]+[int](100*$scale),[int](300*$scale),[int](250*$scale))
# Place the caret after the fixture, clear of the three rows' ink.
[System.Windows.Forms.SendKeys]::SendWait('^{END}')
$frame=Capture-Images "$SelectionEnding-idle.png"
try { $rows=[PlainSelection]::Ink($frame,$area) } finally {$frame.Dispose()}
if ($rows.Count -ne 3) { throw "Expected three text rows, observed $($rows.Count) in $area" }
function GlyphBoundary($Row,$Column,$Length) {
    [Drawing.Point]::new($Row.Left+[int]($Row.Width*$Column/$Length),$Row.Top+[int]($Row.Height/2))
}
$first=GlyphBoundary $rows[0] 2 8
$last=GlyphBoundary $rows[2] 5 7
$expected=@($fixtureRows[0].Substring(2), '', $fixtureRows[2], $fixtureRows[3], $fixtureRows[4].Substring(0,5)) -join $break
$errors=@()
foreach ($direction in 'down','up') {
    if ($direction -eq 'down') { $from=$first; $to=$last } else { $from=$last; $to=$first }
    [PlainSelection]::Drag($from,$to)
    [System.Windows.Forms.Clipboard]::SetText('pending copy')
    [System.Windows.Forms.SendKeys]::SendWait('^c')
    Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -cne 'pending copy' } 'Mouse drag produced no clipboard payload'
    $copied=[System.Windows.Forms.Clipboard]::GetText()
    $frame=Capture-Images "$SelectionEnding-$direction.png"
    try {
        $inkY=@($rows | ForEach-Object { $_.Top+[int]($_.Height/2) })
        # Check that selected text actually has tint. Then scan the full width
        # of each blank visual row at its center, independent of selected ink.
        $measurements=foreach ($i in 0..4) {
            $y=if ($i % 2 -eq 0) { $inkY[$i/2] } else { [int](($inkY[($i-1)/2]+$inkY[($i+1)/2])/2) }
            $band=[PlainSelection]::TintBounds($frame,[Drawing.Rectangle]::new($area.Left,$y-2,$area.Width,5))
            if ($fixtureRows[$i].Length -eq 0 -and !$band.IsEmpty) { $errors+="Selected pixels on empty row $i ($direction): $band" }
            if ($fixtureRows[$i].Length -gt 0 -and $band.IsEmpty) { $errors+="Missing selection on text row $i ($direction)" }
            @{row=$i; y=$y; x=$band.X; width=$band.Width}
        }
        @{ending=$SelectionEnding;direction=$direction;dpi=$dpi;from="$from";to="$to";source=$text;expected=$expected;clipboard=$copied;bands=$measurements} |
            ConvertTo-Json -Depth 4 | Tee-Object (Join-Path $resultDir "$SelectionEnding-$direction.json")
    } finally { $frame.Dispose() }
    if ($copied -cne $expected) { $errors+="Wrong source range for $direction" }
}
if ((Get-FileHash $note).Hash -ne $before) { throw 'Mouse selection modified source' }
if ($errors) { throw ($errors -join [Environment]::NewLine) }
