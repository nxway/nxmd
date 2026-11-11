$ErrorActionPreference = 'Stop'
$exe = Resolve-Path 'target/release/nxmd.exe'
New-Item -ItemType Directory -Force artifacts | Out-Null
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Visual Studio C++ tools were not found' }
$dumpbin = Get-ChildItem "$vs\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Sort-Object FullName -Descending | Select-Object -First 1
if (!$dumpbin) { throw 'dumpbin.exe was not found' }
$imports = & $dumpbin.FullName /DEPENDENTS $exe 2>&1
if ($LASTEXITCODE -ne 0) { throw 'PE dependency inspection failed' }
$imports | Set-Content artifacts/native-dependencies.txt
$headers = & $dumpbin.FullName /HEADERS $exe 2>&1
if ($LASTEXITCODE -ne 0) { throw 'PE header inspection failed' }
$headers | Set-Content artifacts/pe-headers.txt
if (($headers -join "`n") -notmatch '8664 machine \(x64\)') { throw 'Expected an x64 PE executable' }
$tree = & cargo tree --locked --target x86_64-pc-windows-msvc --edges normal
if ($LASTEXITCODE -ne 0) { throw 'Dependency tree inspection failed' }
$tree | Set-Content artifacts/rust-dependencies.txt
$forbidden = '(?i)\b(electron|chromium|webview2|tauri|wry|cef-sys|libcef|node\.dll)\b'
if ((($imports + $tree) -join "`n") -match $forbidden) { throw "Unexpected browser runtime dependency: $($Matches[0])" }
Copy-Item $exe artifacts/nxmd.exe
Copy-Item LICENSE artifacts/LICENSE.txt
Copy-Item THIRD_PARTY.md artifacts/THIRD_PARTY.md
Copy-Item docs/windows-release.md artifacts/README-WINDOWS.md
Get-FileHash artifacts/nxmd.exe -Algorithm SHA256 | Format-List | Out-String | Set-Content artifacts/SHA256.txt
$report = @(
    'x64 PE headers: verified',
    'Native imports and Windows normal dependency tree: recorded',
    'Electron / Chromium / WebView2 / Tauri / CEF / Node runtime matches: none',
    'Package contains nxmd.exe and documentation; no browser runtime files',
    'Inspect native-dependencies.txt for MSVC runtime requirements on a clean machine'
)
$report | Set-Content artifacts/package-validation.txt
$report | ForEach-Object { Write-Host $_ }
