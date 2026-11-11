# Windows release validation

Download the `nxmd-windows-x64-<commit>` artifact from a completed **Windows release** GitHub Actions run, extract the ZIP, and run `nxmd.exe`. Use a copy of your vault for initial testing. This is an unsigned pre-release application; Windows may show a SmartScreen warning.

The artifact is uploaded after tests, release compilation and dependency inspection, before the GUI smoke test. Check the smoke-test step and the separate `nxmd-windows-validation-<commit>` artifact before treating the run as validated. A downloadable executable does not imply the entire workflow passed.

## Runtime requirements

* Windows 10/11 x64 is the intended desktop target. CI uses `windows-latest` (Windows Server); this does not establish Windows 10 compatibility.
* GPUI uses native Windows rendering and text APIs. A compatible graphics driver and Direct3D support are required. A hosted runner may have different graphics capabilities from a desktop.
* The default Rust MSVC build may require the **Microsoft Visual C++ Redistributable 2015–2022 x64**. The artifact's `native-dependencies.txt` records exact imported DLLs; the hosted runner already has development runtimes and cannot establish clean-machine availability. Install Microsoft's official x64 redistributable if Windows reports missing `VCRUNTIME140*.dll` or `MSVCP140*.dll`: https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist
* No Rust installation, Node.js, Electron, Chromium, WebView2 runtime, or bundled browser engine should be required. The package step inspects PE imports and the Windows runtime dependency tree and rejects known browser dependencies. GitHub's own JavaScript-based automation actions run only on the build machine.

## Reproduce

Install Rust stable x64 MSVC, Visual Studio C++ Build Tools and Windows SDK:

```powershell
cargo test --locked --no-default-features
cargo build --locked --release
./scripts/windows-package.ps1
./scripts/windows-smoke.ps1 -Executable ./target/release/nxmd.exe
```

The smoke test uses a temporary vault and isolated LOCALAPPDATA. It checks native window creation, responsiveness, typing and selection undo/redo, targeted context actions, mouse/middle tab closing, external-conflict close protection, unchanged note bytes and graceful exit. It does not certify IME, clipboard, drop behavior or exhaustive editing correctness. No personal vault is used.

`artifacts/` contains the executable, its SHA-256 digest, commit, compiler/runner details, native import report and Windows Rust dependency tree. Workflow logs contain compilation and test output. Source licenses are retained; the package includes nxmd and direct editor attribution. Full transitive redistribution notices should be audited before public distribution.

## Dialog crash fix and diagnostics

File/folder pickers and the trash confirmation now use GPUI's asynchronous native
prompt APIs. On Windows, GPUI owns the dialog, serializes requests and runs the
modal loop on its dedicated COM apartment thread. nxmd no longer invokes rfd
synchronously while a GPUI event handler holds application state.

Diagnostic files are written per process under `%LOCALAPPDATA%\nxmd\logs`.
They include startup, dialog request/completion and error messages. Rust panics
include a forced backtrace. A best-effort native unhandled-exception handler
records the exception code to a pre-opened file without allocating, then leaves
normal Windows crash handling in place. Forced termination, stack exhaustion or
severe corruption cannot be guaranteed to produce a log. These files are local;
they are not uploaded by the application. They are not automatically rotated.

The Windows smoke test exercises all six dialog entry points, accepts a vault
and a new note through the real native dialogs, checks cancellation and owner
re-enabling, verifies unchanged existing note bytes, and checks persistent panic
logging using a separate `nxmd.exe --test-panic-log` process. That diagnostic
switch intentionally exits with a panic; it is not a normal launch command.
