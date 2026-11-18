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
$preservedHash=(Get-FileHash $preservedPath).Hash
function Check-EditorSource([string]$Expected,[string]$Path,[string]$Message) {
    [System.Windows.Forms.Clipboard]::SetText([Guid]::NewGuid().ToString())
    Assert-EditorText $Expected
    [System.Windows.Forms.SendKeys]::SendWait('^s')
    Wait-SavedText $Path $Expected $Message
}
function Open-EditorFixture([int]$Row,[string]$Name) {
    [NativeSmoke]::Click($mainWindow,100,(119+30*$Row))
    Wait-ImageCondition { (Active-ImagePath) -eq $Name } "Editor fixture did not open: $Name"
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
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}{RIGHT 8}+{RIGHT}')
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
[System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
$shot=Capture-Images 'editor-table-narrow.png';$shot.Dispose()
[void][EditorSmokeWindow]::MoveWindow($mainWindow,0,0,1100,760,$true)
Open-EditorFixture 3 '04 Preserved.md'
[System.Windows.Forms.SendKeys]::SendWait('^s')
Wait-ImageCondition { (Get-FileHash $preservedPath).Hash -eq $preservedHash } 'Unchanged BOM/CRLF/YAML preservation'
[System.Windows.Forms.SendKeys]::SendWait('^{END}x^s')
Wait-SavedText $preservedPath ($preserved+'x') 'Editing BOM/CRLF fixture changed unrelated source'
[System.Windows.Forms.SendKeys]::SendWait('^z^s')
Wait-ImageCondition { (Get-FileHash $preservedPath).Hash -eq $preservedHash } 'Undo did not restore exact BOM/CRLF/YAML bytes'
# Close and reopen to verify disk -> fresh editor preservation too.
[System.Windows.Forms.SendKeys]::SendWait('^w')
Wait-ImageCondition { (Active-ImagePath) -ne '04 Preserved.md' } 'Preservation fixture did not close'
Open-EditorFixture 3 '04 Preserved.md'
[System.Windows.Forms.SendKeys]::SendWait('^{END}x^s')
Wait-SavedText $preservedPath ($preserved+'x') 'Reopened fixture changed unrelated source'
[System.Windows.Forms.SendKeys]::SendWait('^z^s')
Wait-ImageCondition { (Get-FileHash $preservedPath).Hash -eq $preservedHash } 'Reopened fixture undo lost original bytes'
'PASS: rapid formatting-boundary navigation, typing/delete/backspace, multi-block selection, undo/redo, tab selection/history, Unicode clipboard/grapheme deletion, table cell navigation/editing at wide and narrow widths, exact BOM/CRLF/YAML save and reopen.' | Tee-Object (Join-Path $resultDir 'editor-result.txt')
