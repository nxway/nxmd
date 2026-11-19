# Real input and native framebuffer checks. All edits target a disposable vault.
$qaVault=New-Item -ItemType Directory -Path (Join-Path $root 'stabilization-vault')
$plain='ANCHOR-B: Привет, мир! 你好世界! café é 👩🏽‍💻.'
$item='      - Alpha ' + ('wrapped selection and indentation ' * 7) + 'end'
$geometry=$plain+"`n"+$item
$geoPath=Join-Path $qaVault '01 Geometry.md'
$keysPath=Join-Path $qaVault '02 Breaks.md'
$tableTest=Join-Path $qaVault '03 Table.md'
$enc=[Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($geoPath,$geometry,$enc)
[IO.File]::WriteAllText($keysPath,'7) [x] Task',$enc)
$tallPath=Join-Path $qaVault '04 Tall.md'
$tall="| Exercise name | Repetitions | Notes |`n| --- | --- | --- |`n"+((1..65 | ForEach-Object {"| Row $_ exercise | 10 | Maintain control |"}) -join "`n")
[IO.File]::WriteAllText($tallPath,$tall,$enc)
$mixedPath=Join-Path $qaVault '05 Mixed.md'
$mixedBlocks=@(
    '# Selection heading',
    '> Quoted words with **bold** and *italic* text',
    '- An unordered item',
    '  - Nested item with `inline code`',
    '7) An ordered item',
    '- [x] Completed task',
    'A closing paragraph.'
) -join "`n"
[IO.File]::WriteAllText($mixedPath,$mixedBlocks,$enc)
Copy-Item (Join-Path $PSScriptRoot '../tests/fixtures/manual-table-lab.md') $tableTest
$tableHash=(Get-FileHash $tableTest).Hash
Add-Type -ReferencedAssemblies @('System.Drawing.Common','System.Drawing.Primitives','System.Runtime.InteropServices') -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public static class SelectionQA {
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h,ref Point p);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h,int msg,IntPtr w,IntPtr l);
    static IntPtr Pos(int x,int y) => (IntPtr)((y<<16)|(x&65535));
    static Point Origin(IntPtr h) {var p=new Point();ClientToScreen(h,ref p);return p;}
    public static void Drag(IntPtr h,int left,int right,int y){
        var o=Origin(h); left-=o.X;right-=o.X;y-=o.Y;
        SendMessage(h,0x200,IntPtr.Zero,Pos(left,y)); SendMessage(h,0x201,(IntPtr)1,Pos(left,y));
        for(int x=left;x<right;x+=5)SendMessage(h,0x200,(IntPtr)1,Pos(x,y));
        SendMessage(h,0x200,(IntPtr)1,Pos(right,y)); SendMessage(h,0x202,IntPtr.Zero,Pos(right,y));
    }
    public static void Wheel(IntPtr h,int steps){
        var o=Origin(h);
        SendMessage(h,0x20a,(IntPtr)(steps*120<<16),Pos(o.X+475,o.Y+210));
    }
    static bool Tint(Color c) => Math.Abs(c.R-64)<=3 && Math.Abs(c.G-74)<=3 && Math.Abs(c.B-61)<=3;
    // Compare the painted selection with the union of glyph ink over a complete
    // visual line. A scanline through an accent alone cannot locate its text.
    public static int[] Bands(Bitmap b,IntPtr h,int width,bool wrapped){
        var o=Origin(h);int x0=o.X+263, x1=Math.Min(b.Width,o.X+width-12);
        int count=0,firstL=0,firstR=0,firstY=0;
        int l=x1,r=-1,inkL=x1,inkR=-1,top=-1,last=-1;
        for(int y=Math.Max(0,o.Y+52);y<Math.Min(b.Height,o.Y+490);y++){
            int left=x1,right=-1;
            for(int x=x0;x<x1;x++)if(Tint(b.GetPixel(x,y))){left=Math.Min(x,left);right=x;}
            int il=x1,ir=-1;
            if(right-left>=12)for(int x=left;x<=right;x++){
                var c=b.GetPixel(x,y);
                if(c.R>100 && c.G>105){il=Math.Min(il,x);ir=x;}
            }
            if(ir>=il){
                if(top<0)top=y;
                last=y;l=Math.Min(l,left);r=Math.Max(r,right);inkL=Math.Min(inkL,il);inkR=Math.Max(inkR,ir);
            }else if(last>=0 && y-last>=4){
                if(inkL-l>15 || r-inkR>15)throw new Exception("Highlight extends into empty space at y="+top+" selected="+l+".."+r+" ink="+inkL+".."+inkR);
                if(count++==0){firstL=l;firstR=r+1;firstY=(top+last)/2;}
                top=last=-1;l=inkL=x1;r=inkR=-1;
            }
        }
        if(count<(wrapped?3:1))throw new Exception("Too few native highlight bands: "+count);
        return new[]{firstL,firstR,firstY,count};
    }
    public static int[] Rails(Bitmap b,IntPtr h){
        var o=Origin(h);int[] xs=new int[4];int count=0;
        for(int x=o.X+278;x<Math.Min(o.X+1090,b.Width);x++){
            int votes=0;
            for(int y=o.Y+260;y<o.Y+500;y++){
                var c=b.GetPixel(x,y);
                // A hovered row/column uses the brighter green frame.
                if(c.G>50 && c.G<220 && c.R>40 && c.G>=c.R && c.B<=c.G) votes++;
            }
            if(votes>190 && (count==0 || x-xs[count-1]>3)){
                if(count==4)throw new Exception("Unexpected additional grid boundary at x="+x);
                xs[count++]=x;
            }
        }
        if(count!=4)throw new Exception("Expected four table boundaries, observed "+count);
        return xs;
    }
    public static void CheckCells(Bitmap b,IntPtr h,int[] rails){
        var o=Origin(h);int ink=0,excess=0;
        // Body column 2 contains only "10", left aligned. Its header sets a
        // wider column. Selection must cover the digits and leave its tail blank.
        for(int y=o.Y+260;y<o.Y+450;y++){
            for(int x=rails[1]+8;x<rails[1]+30;x++)if(Tint(b.GetPixel(x,y)))ink++;
            for(int x=rails[2]-22;x<rails[2]-8;x++)if(Tint(b.GetPixel(x,y)))excess++;
        }
        if(ink<200 || excess>0)throw new Exception("Table selection disagrees with numeric cell content: selected="+ink+", blank padding="+excess);
    }
    public static int BodyChanges(Bitmap before,Bitmap after,IntPtr h,int[] rails){
        var o=Origin(h);int changed=0;
        for(int y=o.Y+260;y<o.Y+490;y+=2)for(int x=rails[0]+6;x<rails[1]-5;x+=2)
            if(before.GetPixel(x,y)!=after.GetPixel(x,y))changed++;
        return changed;
    }
    public static int Grid(Bitmap b,IntPtr h,int width){
        var o=Origin(h);int found=0;
        for(int y=Math.Max(0,o.Y+65);y<Math.Min(b.Height,o.Y+590);y++){
            int run=0;
            for(int x=o.X+275;x<Math.Min(b.Width,o.X+width-15);x++){
                var c=b.GetPixel(x,y);
                if(c.G>50 && c.G<145 && c.R>40 && c.G>=c.R && c.B<=c.G){if(++run==110){found++;break;}}
                else run=0;
            }
        }
        return found;
    }
}
'@
Open-Dialog '^o'; Select-Path $qaVault.FullName $true
Open-EditorFixture 0 '01 Geometry.md'
[void][EditorSmokeWindow]::MoveWindow($mainWindow,0,0,850,760,$true)
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
[System.Windows.Forms.SendKeys]::SendWait('+{END}')
[System.Windows.Forms.Clipboard]::SetText('before plain selection')
[System.Windows.Forms.SendKeys]::SendWait('^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $plain } 'Plain Unicode source selection mismatch'
$shot=Capture-Images 'qa-selection-plain.png'
try {$edges=[SelectionQA]::Bands($shot,$mainWindow,850,$false)} finally {$shot.Dispose()}
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
[SelectionQA]::Drag($mainWindow,$edges[0],$edges[1],$edges[2])
[System.Windows.Forms.Clipboard]::SetText('before drag copy')
[System.Windows.Forms.SendKeys]::SendWait('^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $plain } 'Mouse selection and visible Unicode endpoints disagree'
# Force OpenClipboard to fail for the full bounded retry period. Wait for
# positive acknowledgement that the app attempted Cut while the lock was held.
$lockReady=Join-Path $root 'clipboard-lock-owner.txt'
$lockRelease=Join-Path $root 'release-clipboard-lock'
$holder=Start-Job -ArgumentList $lockReady,$lockRelease -ScriptBlock {
    param($Ready,$Release)
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class ClipboardHolder {
    [DllImport("user32.dll")] public static extern bool OpenClipboard(IntPtr owner);
    [DllImport("user32.dll")] public static extern bool CloseClipboard();
}
"@
    $window=[System.Windows.Forms.NativeWindow]::new()
    $options=[System.Windows.Forms.CreateParams]::new()
    $options.Parent=[IntPtr]::new(-3) # HWND_MESSAGE, never activates the desktop
    $window.CreateHandle($options)
    try {
        if (![ClipboardHolder]::OpenClipboard($window.Handle)) {throw 'Could not hold clipboard'}
        try {
            [IO.File]::WriteAllText($Ready,$window.Handle.ToInt64().ToString())
            $end=[DateTime]::UtcNow.AddSeconds(40)
            while (!(Test-Path $Release) -and [DateTime]::UtcNow -lt $end) {Start-Sleep -Milliseconds 50}
        } finally {[void][ClipboardHolder]::CloseClipboard()}
    } finally {$window.DestroyHandle()}
}
try {
    Wait-ImageCondition { (Test-Path $lockReady) -and [NativeSmoke]::GetOpenClipboardWindow().ToInt64().ToString() -ceq [IO.File]::ReadAllText($lockReady) } 'External clipboard holder did not acquire its window lock'
    if ([NativeSmoke]::OpenClipboard([IntPtr]::Zero)) {
        [void][NativeSmoke]::CloseClipboard()
        throw 'Exclusive clipboard contention was not established'
    }
    [System.Windows.Forms.SendKeys]::SendWait('^x')
    Wait-ImageCondition {
        $currentLog=Get-ChildItem (Join-Path $config 'logs') -Filter "*-$($process.Id).log"
        $null -ne ($currentLog | Select-String -SimpleMatch 'Clipboard write failed; selection retained')
    } 'Locked clipboard Cut was not acknowledged'
} finally {
    [IO.File]::WriteAllText($lockRelease,'release')
    Wait-Job $holder -Timeout 10 | Out-Null
    Receive-Job $holder -ErrorAction Continue
    Remove-Job $holder -Force
}
if ([IO.File]::ReadAllText($geoPath) -cne $geometry) {throw 'Failed Cut modified the file'}
[System.Windows.Forms.Clipboard]::SetText('after failed cut')
[System.Windows.Forms.SendKeys]::SendWait('^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $plain } 'Failed Cut lost selected source or range'
[System.Windows.Forms.SendKeys]::SendWait('^x')
Wait-SavedText $geoPath ("`n"+$item) 'Drag cut removed wrong bytes'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Wait-SavedText $geoPath $geometry 'Drag cut undo changed source'
[System.Windows.Forms.SendKeys]::SendWait('^a')
[System.Windows.Forms.SendKeys]::SendWait('^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $geometry } 'Wrapped selection clipboard mismatch'
$shot=Capture-Images 'qa-selection-wrapped-list.png'
try { [void][SelectionQA]::Bands($shot,$mainWindow,850,$true) } finally {$shot.Dispose()}

Open-EditorFixture 1 '02 Breaks.md'
[System.Windows.Forms.SendKeys]::SendWait('^{END}+{ENTER}continued')
$continued="7) [x] Task  `n       continued"
Check-EditorSource $continued $keysPath 'Hard break inside numbered task'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{END}{ENTER}Next')
$split="7) [x] Task  `n8) [ ] Next`n       continued"
Check-EditorSource $split $keysPath 'Ordered task continuation'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT}')
[System.Windows.Forms.SendKeys]::SendWait('{BACKSPACE}9')
Check-EditorSource ($split.Replace('7)','9)')) $keysPath 'Editing original ordered marker'
# Normal hard break replaces a selected word, then survives close and reopen.
[System.Windows.Forms.SendKeys]::SendWait('^a')
[System.Windows.Forms.SendKeys]::SendWait('Plain words')
[System.Windows.Forms.SendKeys]::SendWait('^+{LEFT}')
[System.Windows.Forms.SendKeys]::SendWait('+{ENTER}')
Check-EditorSource "Plain  `n" $keysPath 'Hard break with selected word'
[System.Windows.Forms.SendKeys]::SendWait('^w')
Open-EditorFixture 1 '02 Breaks.md'
Check-EditorSource "Plain  `n" $keysPath 'Reopened hard break'
Open-EditorFixture 2 '03 Table.md'
foreach ($width in 850,1100) {
    [void][EditorSmokeWindow]::MoveWindow($mainWindow,0,0,$width,760,$true)
    [System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
    foreach ($cycle in 1..3) {
        [SelectionQA]::Wheel($mainWindow,-18)
        Start-Sleep -Milliseconds 250
        [SelectionQA]::Wheel($mainWindow,36)
        Start-Sleep -Milliseconds 350
        $shot=Capture-Images "qa-grid-$width-$cycle.png"
        try {if ([SelectionQA]::Grid($shot,$mainWindow,$width) -lt 4){throw "Missing QA table borders after scrolling at width $width"}} finally {$shot.Dispose()}
    }
    Open-EditorFixture 0 '01 Geometry.md'
    Open-EditorFixture 2 '03 Table.md'
}
if ((Get-FileHash $tableTest).Hash -ne $tableHash){throw 'Scrolling or switching tabs changed QA table source'}
Open-EditorFixture 3 '04 Tall.md'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
Start-Sleep -Milliseconds 400
$shot=Capture-Images 'qa-tall-grid-before.png'
try {
    $rails=[SelectionQA]::Rails($shot,$mainWindow)
    [SelectionQA]::Wheel($mainWindow,-12)
    Start-Sleep -Milliseconds 450
    $after=Capture-Images 'qa-tall-grid-scrolled.png'
    try {
        $scrolledRails=[SelectionQA]::Rails($after,$mainWindow)
        foreach ($i in 0..3) {
            if ([Math]::Abs($rails[$i]-$scrolledRails[$i]) -gt 2) {throw 'Table frame moved or disappeared while its header was above the viewport'}
        }
        if ([SelectionQA]::BodyChanges($shot,$after,$mainWindow,$rails) -lt 150) {throw 'Tall table did not visibly scroll'}
    } finally {$after.Dispose()}
} finally {$shot.Dispose()}
[System.Windows.Forms.SendKeys]::SendWait('^{END}^+{HOME}^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $tall } 'Table selection source mismatch'
Start-Sleep -Milliseconds 200
$shot=Capture-Images 'qa-table-selected.png'
try {[SelectionQA]::CheckCells($shot,$mainWindow,$rails)} finally {$shot.Dispose()}
Open-EditorFixture 4 '05 Mixed.md'
[System.Windows.Forms.SendKeys]::SendWait('^a^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $mixedBlocks } 'Mixed Markdown selection source mismatch'
$shot=Capture-Images 'qa-selection-mixed.png'
try {
    $mixedBands=[SelectionQA]::Bands($shot,$mainWindow,1100,$true)
    if ($mixedBands[3] -lt 7) {throw 'Heading, quote, list, nested item, ordinal, task or paragraph has no visible selection'}
} finally {$shot.Dispose()}
if ([IO.File]::ReadAllText($mixedPath) -cne $mixedBlocks) {throw 'Selecting mixed Markdown changed source'}
'PASS: native mouse drag/cut/undo, Unicode clipboard, wrapped highlight geometry, hard breaks, marker editing, QA table border checks across wheel scrolls and widths.' | Tee-Object (Join-Path $resultDir 'stabilization-result.txt')
