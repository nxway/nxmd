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
    // Measure actual selection bands. Each group of consecutive tinted scanlines
    // spans one or more wrapped lines. Test each text baseline separately.
    public static int[] Bands(Bitmap b,IntPtr h,int width,bool wrapped){
        var o=Origin(h);int x0=o.X+263, x1=Math.Min(b.Width,o.X+width-12);
        int previous=-1,count=0,firstL=0,firstR=0,firstY=0;
        for(int y=Math.Max(0,o.Y+52);y<Math.Min(b.Height,o.Y+480);y++){
            int l=x1,r=-1;
            for(int x=x0;x<x1;x++){
                var c=b.GetPixel(x,y);
                if(Math.Abs(c.R-64)<=3 && Math.Abs(c.G-73)<=3 && Math.Abs(c.B-61)<=3){l=Math.Min(x,l);r=x;}
            }
            if(r-l<12)continue;
            // A row centre has glyph ink. Combine +/- 4 scanlines so glyph
            // ascenders and descenders contribute to the horizontal envelope.
            int inkL=x1,inkR=-1;
            for(int x=l;x<=r;x++)for(int j=-4;j<=4;j++){
                var c=b.GetPixel(x,y+j);
                if(c.R>140 && c.G>140){inkL=Math.Min(inkL,x);inkR=Math.Max(inkR,x);break;}
            }
            if(inkR-inkL<12)continue;
            if(previous>=0 && y-previous<20)continue;
            if(inkL-l>15 || r-inkR>15)throw new Exception("Highlight extends into empty space at y="+y+" selected="+l+".."+r+" ink="+inkL+".."+inkR);
            if(count++==0){firstL=l;firstR=r+1;firstY=y;}
            previous=y;
        }
        if(count<(wrapped?3:1))throw new Exception("Too few native highlight bands: "+count);
        return new[]{firstL,firstR,firstY,count};
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
'PASS: native mouse drag/cut/undo, Unicode clipboard, wrapped highlight geometry, hard breaks, marker editing, QA table border checks across wheel scrolls and widths.' | Tee-Object (Join-Path $resultDir 'stabilization-result.txt')
