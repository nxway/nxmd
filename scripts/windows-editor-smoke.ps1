# Dot-sourced after windows-images-smoke.ps1: reuse native input/observed-state helpers.
$editorVault=New-Item -ItemType Directory -Force (Join-Path $root 'editor-vault')
$preview="Start **bold**normal and ***both***.`n`n7. First item`n8. Second item`n   4. Nested item`n   5. Another nested item`n9. Third item`n`n**bold with *nested italic* inside**`n*italic with **bold** inside*`n~~strikethrough~~ and ``**literal**```n`n> Quote`n`n- [x] Done`n- [ ] Pending`n`nEnd."
$table='| Name | Details | Right |' + "`n" + '| :--- | :---: | ---: |' + "`n" + '| **Bold** | A longer sentence with ordinary words that should wrap naturally when the window is narrow. | Café |' + "`n" + '| *Italic* | [Link](https://example.com) and `code` | 日本語 🙂 |' + "`n" + '| Escaped \| pipe | Привет | Last |' + "`n`nEnd."
$preserved="---`r`ntitle: 'Keep  spaces'`r`nunknown: [a, b]`r`n---`r`n`r`n07) Original  `r`n9) Ordinal`r`n`r`n" + $table.Replace("`n","`r`n") + "`r`n![[missing.png|80]]`r`n[^unsupported]: untouched`r`n"
$previewPath=Join-Path $editorVault '01 Preview.md'
$tablePath=Join-Path $editorVault '02 Table.md'
$preservedPath=Join-Path $editorVault '04 Preserved.md'
[IO.File]::WriteAllText($previewPath,$preview,[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($tablePath,$table,[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $editorVault '03 Other.md'),'Other tab',[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($preservedPath,$preserved,[Text.UTF8Encoding]::new($true))
$wrappedList='7. '+('A long list item with ordinary words and **formatting** to exercise wrapped navigation. '*10)+"`n8. Second item`n`nAfter."
$wrappedListPath=Join-Path $editorVault '05 Wrapped.md'
[IO.File]::WriteAllText($wrappedListPath,$wrappedList,[Text.UTF8Encoding]::new($false))
$mixedEnds = "one`r`ntwo`n" + 'a$x$b'
$mixedPath = Join-Path $editorVault '06 Mixed.md'
[IO.File]::WriteAllText($mixedPath,$mixedEnds,[Text.UTF8Encoding]::new($false))
$preservedHash=(Get-FileHash $preservedPath).Hash
function Check-EditorSource([string]$Expected,[string]$Path,[string]$Message) {
    [System.Windows.Forms.Clipboard]::SetText([Guid]::NewGuid().ToString())
    Assert-EditorText $Expected
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    Wait-SavedText $Path $Expected $Message
}
Open-Dialog '^o'; Select-Path $editorVault.FullName $true
Open-EditorFixture 0 '01 Preview.md'
Check-EditorSource $preview $previewPath 'Initial Markdown copy/save changed source'
# Events are sent once, without sleeps between arrows. Input must be correct
# even when several events arrive before the next render updates hidden maps.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 8}X')
$inserted=$preview.Replace('**bold**normal','**Xbold**normal')
Check-EditorSource $inserted $previewPath 'Typing inside revealed bold changed the wrong source position'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Inline insertion undo'
[System.Windows.Forms.SendKeys]::SendWait('^+z')
Check-EditorSource $inserted $previewPath 'Inline insertion redo'
# Keep a one-character selection and undo history while switching entities/tabs.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 8}+{RIGHT}^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq 'X' } 'Selection was not acknowledged before switching tabs'
Open-EditorFixture 2 '03 Other.md'
Open-EditorFixture 0 '01 Preview.md'
[System.Windows.Forms.SendKeys]::SendWait('R')
Check-EditorSource ($inserted.Replace('**Xbold','**Rbold')) $previewPath 'Tab switch lost selected range'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $inserted $previewPath 'Tab switch lost undo history'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Prior edit history lost after tab switch'
# Delete a content character, then a visible delimiter: both are source edits.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 8}{DELETE}')
Check-EditorSource ($preview.Replace('**bold**normal','**old**normal')) $previewPath 'Delete at formatting boundary'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Boundary delete undo'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 8}{BACKSPACE}')
Check-EditorSource ($preview.Replace('**bold**normal','*bold**normal')) $previewPath 'Backspace must edit the revealed delimiter'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Delimiter deletion undo'
# Multiline selection crossing list and formatting boundaries, then exact undo.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 6}^+{END}{DELETE}')
Check-EditorSource 'Start ' $previewPath 'Multiblock selection deletion'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Multiblock selection undo'
# Native Unicode clipboard path: Cyrillic, emoji, and a decomposed accent.
$unicode=' Привет 日本語 🙂 e' + [char]0x301
[System.Windows.Forms.Clipboard]::SetText($unicode)
[System.Windows.Forms.SendKeys]::SendWait('^{END}^v')
# Observe autosave before replacing clipboard data for the copy checkpoint.
Wait-SavedText $previewPath ($preview+$unicode) 'Unicode paste/autosave'
Check-EditorSource ($preview+$unicode) $previewPath 'Unicode copy round trip'
[System.Windows.Forms.SendKeys]::SendWait('{BACKSPACE}')
Check-EditorSource ($preview+' Привет 日本語 🙂 ') $previewPath 'Backspace split a composed grapheme'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource ($preview+$unicode) $previewPath 'Unicode grapheme deletion undo'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $preview $previewPath 'Unicode paste undo'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
$shot=Capture-Images 'editor-list-inline.png';$shot.Dispose()
Open-EditorFixture 1 '02 Table.md'
Check-EditorSource $table $tablePath 'Table copy/save must preserve pipes and markers'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{RIGHT 2}X')
$tableEdit=$table.Replace('**Bold**','**XBold**')
Check-EditorSource $tableEdit $tablePath 'Table caret mapping inserted in the wrong cell/source position'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $table $tablePath 'Table edit undo changed unrelated source'
# A byte column of two in Latin text would land inside the first Japanese
# character without UTF-8 clamping when moving to the next row.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{TAB}{TAB}{RIGHT 2}{DOWN}Q')
Check-EditorSource ($table.Replace('日本語 🙂','Q日本語 🙂')) $tablePath 'Cross-cell vertical movement split Unicode'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $table $tablePath 'Unicode table navigation edit undo'
# Tab is cell navigation. Repeated vertical movement must not split Unicode.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{TAB}{TAB}{DOWN}{LEFT}{UP}{DOWN}')
Check-EditorSource $table $tablePath 'Table navigation changed source or failed on Unicode'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
$shot=Capture-Images 'editor-table-wide.png';$shot.Dispose()
# Resize the real native window; row geometry and inline runs must still edit.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class EditorSmokeWindow {
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int width,int height,bool repaint);
}
'@
[void][EditorSmokeWindow]::MoveWindow($mainWindow,0,0,760,900,$true)
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{RIGHT 2}X')
Check-EditorSource $tableEdit $tablePath 'Narrow table caret mapping'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $table $tablePath 'Narrow table undo'
# Down must visit the second wrapped visual line of Details, not the next row.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{TAB}{DOWN}Z^s')
$wrappedEdit=Wait-SavedInsertion $tablePath 'Z'
$at=$wrappedEdit.IndexOf('Z')
$sentenceStart=$table.IndexOf('A longer sentence')
$sentenceEnd=$table.IndexOf(' | Café')
if ($wrappedEdit.Remove($at,1) -cne $table -or $at -le $sentenceStart -or $at -ge $sentenceEnd) {
    throw 'Down skipped a wrapped table line or edited outside the intended cell'
}
Check-EditorSource $wrappedEdit $tablePath 'Wrapped table source mapping'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $table $tablePath 'Wrapped table movement edit undo'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}{TAB}{DOWN 2}Y^s')
$secondEdit=Wait-SavedInsertion $tablePath 'Y'
$secondAt=$secondEdit.IndexOf('Y')
if ($secondEdit.Remove($secondAt,1) -cne $table -or $secondAt -le $at -or $secondAt -ge $sentenceEnd) {
    throw 'Repeated Down did not progress through the wrapped cell'
}
Check-EditorSource $secondEdit $tablePath 'Repeated wrapped table movement'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $table $tablePath 'Repeated wrapped movement undo'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
$shot=Capture-Images 'editor-table-narrow.png';$shot.Dispose()
[void][EditorSmokeWindow]::MoveWindow($mainWindow,0,0,1100,760,$true)
Open-EditorFixture 3 '04 Preserved.md'
[System.Windows.Forms.SendKeys]::SendWait('^s')
Wait-ImageCondition { (Read-SmokeSnapshot $preservedPath -Hash) -eq $preservedHash } 'Unchanged BOM/CRLF/YAML preservation'
[System.Windows.Forms.SendKeys]::SendWait('^{END}x^s')
Wait-SavedText $preservedPath ($preserved+'x') 'Editing BOM/CRLF fixture changed unrelated source'
[System.Windows.Forms.SendKeys]::SendWait('^z')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Wait-ImageCondition { (Read-SmokeSnapshot $preservedPath -Hash) -eq $preservedHash } 'Undo did not restore exact BOM/CRLF/YAML bytes'
# Close and reopen to verify disk -> fresh editor preservation too.
[System.Windows.Forms.SendKeys]::SendWait('^w')
Wait-ImageCondition { (Active-ImagePath) -ne '04 Preserved.md' } 'Preservation fixture did not close'
Open-EditorFixture 3 '04 Preserved.md'
[System.Windows.Forms.SendKeys]::SendWait('^{END}x^s')
Wait-SavedText $preservedPath ($preserved+'x') 'Reopened fixture changed unrelated source'
[System.Windows.Forms.SendKeys]::SendWait('^z')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Wait-ImageCondition { (Read-SmokeSnapshot $preservedPath -Hash) -eq $preservedHash } 'Reopened fixture undo lost original bytes'
Open-EditorFixture 4 '05 Wrapped.md'
Check-EditorSource $wrappedList $wrappedListPath 'Wrapped list original ordinals'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{DOWN}Z^s')
$listEdit=Wait-SavedInsertion $wrappedListPath 'Z'
$at=$listEdit.IndexOf('Z')
if ($listEdit.Remove($at,1) -cne $wrappedList -or $at -le 3 -or $at -ge $wrappedList.IndexOf("`n")) {
    throw 'Down skipped a wrapped list line or changed its marker'
}
Check-EditorSource $listEdit $wrappedListPath 'Wrapped list visual/source mapping'
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $wrappedList $wrappedListPath 'Wrapped list edit undo'
# Inserting an item may add its marker, but must not rewrite later ordinals.
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{END}{ENTER}New')
$listInserted=$wrappedList.Replace("`n8. Second", "`n8. New`n8. Second")
Check-EditorSource $listInserted $wrappedListPath 'List continuation rewrote unrelated source ordinals'
[System.Windows.Forms.SendKeys]::SendWait('^z')
[System.Windows.Forms.SendKeys]::SendWait('^z')
Check-EditorSource $wrappedList $wrappedListPath 'List structural edit undo'
Open-EditorFixture 5 '06 Mixed.md'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{END}X^s')
Wait-SavedText $mixedPath ('oneX'+$mixedEnds.Substring(3)) 'End must insert before both CRLF bytes'
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}+{END}^c')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq 'oneX' } 'Shift+End copied a line terminator'
[System.Windows.Forms.Clipboard]::SetText('first')
[System.Windows.Forms.SendKeys]::SendWait('^v')
[System.Windows.Forms.SendKeys]::SendWait('^s')
$edited = 'first'+$mixedEnds.Substring(3)
Wait-SavedText $mixedPath $edited 'Paste over first row retained mixed endings'
# Cut and paste the full source through CF_UNICODETEXT, including both separator styles.
[System.Windows.Forms.SendKeys]::SendWait('^a')
[System.Windows.Forms.SendKeys]::SendWait('^x')
Wait-ImageCondition { [System.Windows.Forms.Clipboard]::GetText() -ceq $edited } 'Cut changed mixed line endings'
Wait-SavedText $mixedPath '' 'Cut did not remove the selected text'
[System.Windows.Forms.SendKeys]::SendWait('^v')
[System.Windows.Forms.SendKeys]::SendWait('^s')
Wait-SavedText $mixedPath $edited 'Multiline paste changed mixed line endings'
[System.Windows.Forms.SendKeys]::SendWait('^w')
Wait-ImageCondition { (Active-ImagePath) -ne '06 Mixed.md' } 'Mixed fixture did not close'
Open-EditorFixture 5 '06 Mixed.md'
[System.Windows.Forms.SendKeys]::SendWait('^{END}{LEFT 3}Z^s')
$expected = $edited.Replace('a$x$b','a$Zx$b')
Wait-SavedText $mixedPath $expected 'Reopened mixed fixture and traversed inline math'
$encoded = [Text.UTF8Encoding]::new($false).GetBytes($expected)
if ((Get-FileHash $mixedPath).Hash -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($encoded))) {
    throw 'Reopened note differs from the expected UTF-8 bytes'
}
Open-EditorFixture 4 '05 Wrapped.md'
# Fixed seed and an independent source string make a failure replayable.
$rng=[Random]::new(73217)
$fragments=@(' **bold**',' ~~strike~~',' é',' Привет',' 日本語',' 🙂',' | cell |')
$model=$wrappedList
foreach ($step in 1..16) {
    $fragment=$fragments[$rng.Next($fragments.Count)]
    [System.Windows.Forms.Clipboard]::SetText($fragment)
    [System.Windows.Forms.SendKeys]::SendWait('^{END}^v')
    Wait-SavedText $wrappedListPath ($model+$fragment) "Seed 73217 paste $step"
    if ($rng.Next(2) -eq 0) {
        [System.Windows.Forms.SendKeys]::SendWait('^z')
        [System.Windows.Forms.SendKeys]::SendWait('^s')
        Wait-SavedText $wrappedListPath $model "Seed 73217 undo $step"
        [System.Windows.Forms.SendKeys]::SendWait('^+z')
        [System.Windows.Forms.SendKeys]::SendWait('^s')
        Wait-SavedText $wrappedListPath ($model+$fragment) "Seed 73217 redo $step"
    }
    $model+=$fragment
}
# Exercise both branches of the host callback while the native Move picker is open.
Open-EditorFixture 5 '06 Mixed.md'
function Open-MixedMove {
    [NativeSmoke]::RightClick($mainWindow,80,269)
    Start-Sleep -Milliseconds 200
    [NativeSmoke]::Click($mainWindow,120,322)
    $picker = Wait-Dialog
    Wait-SmokeForeground $picker
    Wait-ImageCondition { [NativeSmoke]::GetDlgItem($picker,1) -ne [IntPtr]::Zero } 'Move picker did not finish opening'
}
$moved = Join-Path $editorVault '07 Moved.md'
$stage = Join-Path $root 'writer-stage.tmp'
$original = Join-Path $root 'original-backup.tmp'
$winner = Join-Path $root 'writer-backup.tmp'
$writerText = "external publication 日本語`r`n"
$originalHash = (Get-FileHash $mixedPath).Hash
[IO.File]::WriteAllText($stage,$writerText,[Text.UTF8Encoding]::new($false))
Open-MixedMove
[IO.File]::Replace($stage,$mixedPath,$original)
Select-Path $moved
Wait-ImageCondition { (Image-Logs).Contains('Source file was replaced while choosing the destination') } 'Move did not detect replacement during picker'
if ((Active-ImagePath) -ne '06 Mixed.md' -or (Test-Path $moved) -or
    [IO.File]::ReadAllText($mixedPath) -cne $writerText -or (Get-FileHash $original).Hash -ne $originalHash) {
    throw 'Failed move changed the active path or the competing bytes'
}
# Restore the test baseline while retaining the winning publication, then retry.
[IO.File]::Replace($original,$mixedPath,$winner)
Open-MixedMove
Select-Path $moved
Wait-ImageCondition { (Active-ImagePath) -eq '07 Moved.md' } 'Successful move did not update the tab path'
if ((Test-Path $mixedPath) -or (Get-FileHash $moved).Hash -ne $originalHash -or
    [IO.File]::ReadAllText($winner) -cne $writerText) {
    throw 'Successful move or retry changed the competing bytes'
}
'PASS: native Move rejected an external replacement during the picker, kept the source tab, then updated the path only after a successful retry.' | Set-Content (Join-Path $resultDir 'move-result.txt')
'PASS: rapid formatting-boundary navigation, typing/delete/backspace, multi-block selection, undo/redo, tab selection/history, Unicode clipboard/grapheme deletion, table cell navigation/editing at wide and narrow widths, exact BOM/CRLF/YAML save and reopen.' | Tee-Object (Join-Path $resultDir 'editor-result.txt')
