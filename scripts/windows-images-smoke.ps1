# Dot-sourced by windows-smoke.ps1; all files live in its temporary test root.
function Wait-ImageCondition([scriptblock]$Condition,[string]$Message) {
    $until=[DateTime]::UtcNow.AddSeconds(20)
    do { Assert-Alive; if (& $Condition) { return }; Start-Sleep -Milliseconds 100 } until ([DateTime]::UtcNow -gt $until)
    throw $Message
}
function Image-Logs { Get-ChildItem (Join-Path $config 'logs') -Filter '*.log' | Get-Content | Out-String }
function Active-ImagePath {
    $s=Get-Content (Join-Path $config 'session.json') -Raw | ConvertFrom-Json
    if ($s.tabs.Count -gt 0) { return $s.tabs[$s.active].Replace('\','/') }
    return ''
}
function Capture-Images([string]$Name) {
    $bounds=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp=[System.Drawing.Bitmap]::new($bounds.Width,$bounds.Height)
    $g=[System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.Location,[System.Drawing.Point]::Empty,$bounds.Size)
    $g.Dispose()
    if ($Name) { $bmp.Save((Join-Path $resultDir.FullName $Name)) }
    return $bmp
}
function Color-Pixels([int]$Red,[int]$Green,[int]$Blue) {
    $bmp=Capture-Images ''
    $count=0
    try {
        for ($x=280;$x -lt $bmp.Width;$x+=3) {
            for ($y=80;$y -lt [Math]::Min(700,$bmp.Height);$y+=3) {
                $p=$bmp.GetPixel($x,$y)
                if ([Math]::Abs($p.R-$Red) -lt 12 -and [Math]::Abs($p.G-$Green) -lt 12 -and [Math]::Abs($p.B-$Blue) -lt 12) { $count++ }
            }
        }
    } finally {$bmp.Dispose()}
    return $count
}
function Write-TestImage([string]$Path,[int]$Width,[int]$Height,[Drawing.Color]$Color) {
    $bmp=[Drawing.Bitmap]::new($Width,$Height);$g=[Drawing.Graphics]::FromImage($bmp)
    $g.Clear($Color);$g.Dispose();$bmp.Save($Path,[Drawing.Imaging.ImageFormat]::Png);$bmp.Dispose()
}
$imagesVault=New-Item -ItemType Directory -Force (Join-Path $root 'images-vault')
foreach($dir in @('Pictures','More','.obsidian')) { New-Item -ItemType Directory -Force (Join-Path $imagesVault $dir) | Out-Null }
Write-TestImage (Join-Path $imagesVault 'Pictures/standard.png') 96 32 ([Drawing.Color]::FromArgb(240,30,70))
Write-TestImage (Join-Path $imagesVault 'More/unique.png') 64 48 ([Drawing.Color]::FromArgb(20,190,230))
Copy-Item (Join-Path $imagesVault 'Pictures/standard.png') (Join-Path $imagesVault 'Pictures/dup.png')
Copy-Item (Join-Path $imagesVault 'More/unique.png') (Join-Path $imagesVault 'More/dup.png')
Copy-Item (Join-Path $imagesVault 'More/unique.png') (Join-Path $imagesVault '.obsidian/private.png')
$imageNote=Join-Path $imagesVault 'ImageNote.md'
$imageSource="# Images`r`n`r`n![](Pictures/standard.png)`r`n`r`n![[unique.png]]`r`n`r`n![[Pictures/standard.png]]`r`n`r`n![[dup.png]]`r`n`r`n![](missing.png)`r`n`r`nEnd."
[IO.File]::WriteAllText($imageNote,$imageSource,[Text.UTF8Encoding]::new($true))
$imageSourceHash=(Get-FileHash $imageNote).Hash
Open-Dialog '^o'; Select-Path $imagesVault.FullName $true
Start-Sleep -Milliseconds 700
[NativeSmoke]::Click($mainWindow,100,179)
Wait-ImageCondition { (Active-ImagePath) -eq 'ImageNote.md' } 'Image fixture note did not open'
Wait-ImageCondition { (Color-Pixels 240 30 70) -gt 250 -and (Color-Pixels 20 190 230) -gt 150 } 'Standard/Obsidian images did not paint in Live Preview'
Wait-ImageCondition { (Image-Logs).Contains('Ambiguous image filename: dup.png') -and (Image-Logs).Contains('Image not found inside vault: missing.png') } 'Missing/ambiguous images did not report useful errors'
if ((Get-FileHash $imageNote).Hash -ne $imageSourceHash) { throw 'Rendering embeds changed BOM/CRLF Markdown bytes' }
$shot=Capture-Images 'image-embeds.png';$shot.Dispose()
# Enter the Obsidian embed's source, explicitly edit and undo, then check exact bytes.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
[System.Windows.Forms.SendKeys]::SendWait('{DOWN 4}')
[System.Windows.Forms.SendKeys]::SendWait('{END}')
[System.Windows.Forms.SendKeys]::SendWait(' ')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Start-Sleep -Milliseconds 300
if ((Get-FileHash $imageNote).Hash -eq $imageSourceHash) { throw 'Caret edit did not reach Obsidian embed source' }
[System.Windows.Forms.SendKeys]::SendWait('^z')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Start-Sleep -Milliseconds 300
if ((Get-FileHash $imageNote).Hash -ne $imageSourceHash) { throw 'Obsidian source edit/undo changed original bytes' }
# Actual Windows bitmap clipboard -> native GPUI paste -> vault-local PNG.
[System.Windows.Forms.SendKeys]::SendWait('^{END}')
$clip=[Drawing.Bitmap]::new(72,48);$g=[Drawing.Graphics]::FromImage($clip)
$g.Clear([Drawing.Color]::Lime);$g.Dispose();[System.Windows.Forms.Clipboard]::SetImage($clip);$clip.Dispose()
$importWatch=[Diagnostics.Stopwatch]::StartNew()
[System.Windows.Forms.SendKeys]::SendWait('^v')
$imported=Join-Path $imagesVault 'attachments/image-1.png'
Wait-ImageCondition { (Test-Path $imported) -and [IO.File]::ReadAllText($imageNote).Contains('![](attachments/image-1.png)') } 'Native clipboard image did not import and autosave'
$importHash=(Get-FileHash $imported).Hash
# attachments must have appeared automatically as the first folder; expand and open.
[NativeSmoke]::Click($mainWindow,100,119)
Start-Sleep -Milliseconds 200
[NativeSmoke]::Click($mainWindow,100,149)
Wait-ImageCondition { (Active-ImagePath) -eq 'attachments/image-1.png' -and (Color-Pixels 0 255 0) -gt 250 } 'Imported image was not visible/openable in explorer'
$importToViewerMs=$importWatch.Elapsed.TotalMilliseconds
[System.Windows.Forms.SendKeys]::SendWait('^s')
if ((Get-FileHash $imported).Hash -ne $importHash) { throw 'Saving an image tab modified binary bytes' }
# Switching back retains the Markdown editor's undo history.
[NativeSmoke]::Click($mainWindow,370,22)
[System.Windows.Forms.SendKeys]::SendWait('^z')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Start-Sleep -Milliseconds 300
if ((Get-FileHash $imageNote).Hash -ne $imageSourceHash) { throw 'Switching image tabs lost Markdown undo/source preservation' }
[System.Windows.Forms.SendKeys]::SendWait('^+z')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Start-Sleep -Milliseconds 300
if (![IO.File]::ReadAllText($imageNote).Contains('![](attachments/image-1.png)')) { throw 'Image insertion redo failed' }
# Native watcher: create/rename/delete externally, with no F5 or vault reopen.
$external=Join-Path $imagesVault '0 External.webp'
Copy-Item (Join-Path $PSScriptRoot '../tests/fixtures/sample.webp') $external
Start-Sleep -Milliseconds 700
[NativeSmoke]::Click($mainWindow,100,239)
Wait-ImageCondition { (Active-ImagePath) -eq '0 External.webp' } 'Externally created image was not shown'
Wait-ImageCondition { (Color-Pixels 240 130 20) -gt 70 } 'WebP viewer did not display its first frame'
Rename-Item $external '0 Renamed.webp'
Start-Sleep -Milliseconds 700
[NativeSmoke]::Click($mainWindow,100,239)
Wait-ImageCondition { (Active-ImagePath) -eq '0 Renamed.webp' } 'Externally renamed image was not shown'
Remove-Item (Join-Path $imagesVault '0 Renamed.webp')
Start-Sleep -Milliseconds 700
[NativeSmoke]::Click($mainWindow,100,239)
Wait-ImageCondition { (Active-ImagePath) -eq 'ImageNote.md' } 'Deleted image row remained in explorer'
# Restore a real image tab through a process restart, without a vault argument.
[NativeSmoke]::Click($mainWindow,550,22)
Wait-ImageCondition { (Active-ImagePath) -eq 'attachments/image-1.png' } 'Cannot activate imported image before restart'
if (!$process.CloseMainWindow() -or !$process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Image-session close failed' }
$process=Start-Process -FilePath $exe -PassThru -RedirectStandardOutput (Join-Path $resultDir 'images-restart-stdout.txt') -RedirectStandardError (Join-Path $resultDir 'images-restart-stderr.txt')
Wait-ImageCondition { $process.Refresh(); $process.MainWindowHandle -ne 0 } 'Image session did not reopen'
$mainWindow=$process.MainWindowHandle
[void][NativeSmoke]::SetForegroundWindow($mainWindow)
Wait-ImageCondition { (Color-Pixels 0 255 0) -gt 250 } 'Restored image tab did not render'
if ((Get-FileHash $imported).Hash -ne $importHash) { throw 'Image changed across restart' }
# Many files: no thumbnail decoding on enumeration, and bounded lazy viewing.
$process.Refresh();$beforeImages=@{workingSetBytes=$process.WorkingSet64;privateBytes=$process.PrivateMemorySize64}
$big=New-Item -ItemType Directory -Force (Join-Path $imagesVault 'Big')
Write-TestImage (Join-Path $big 'img-0001.png') 1024 1024 ([Drawing.Color]::Gold)
for($i=2;$i -le 1500;$i++) { Copy-Item (Join-Path $big 'img-0001.png') (Join-Path $big ('img-{0:D4}.png' -f $i)) }
Start-Sleep -Seconds 2
if ((Image-Logs) -match 'image decoded: Big[/\\]') { throw 'Explorer eagerly decoded images' }
# Collapsed folders after restart: attachments row 0, Big row 1.
[NativeSmoke]::Click($mainWindow,100,149)
Start-Sleep -Milliseconds 300
$loadWatch=[Diagnostics.Stopwatch]::StartNew()
for($i=1;$i -le 12;$i++) {
    [NativeSmoke]::Click($mainWindow,100,(179+30*($i-1)))
    $expected='Big/img-{0:D4}.png' -f $i
    Wait-ImageCondition { (Active-ImagePath) -eq $expected -and ((Image-Logs).Replace('\','/').Contains("image decoded: $expected ")) } "Image viewer did not load $expected"
}
$multipleImageMs=$loadWatch.Elapsed.TotalMilliseconds
$process.Refresh();$afterImages=@{workingSetBytes=$process.WorkingSet64;privateBytes=$process.PrivateMemorySize64}
$expansion=[regex]::Matches((Image-Logs),'explorer toggle: Big rows=(\d+) elapsed_us=(\d+)') | Select-Object -Last 1
if (!$expansion -or [int]$expansion.Groups[1].Value -lt 1500) { throw 'Large directory did not expand through the cached explorer' }
@{importToViewerDisplayMs=[Math]::Round($importToViewerMs,1);cachedTreeExpansionMs=([double]$expansion.Groups[2].Value/1000);imageCount=1500;openedImageCount=12;openTwelveImagesThroughUiMs=[Math]::Round($multipleImageMs,1);beforeImages=$beforeImages;afterImages=$afterImages} | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $resultDir 'image-performance.json')
$shot=Capture-Images 'image-viewer.png';$shot.Dispose()
'PASS: native bitmap paste and autosave; automatic attachment visibility; image viewer and session restore; standard and Obsidian rendering; missing/ambiguous errors; byte-preserving source edit/undo; external image create/rename/delete; 1500-file expansion; 12 lazy image tabs.' | Tee-Object (Join-Path $resultDir 'image-result.txt')
