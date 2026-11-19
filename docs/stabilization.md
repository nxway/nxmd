# Editing and diagnostic checks

The regression fixtures use disposable vaults. To run the Windows GUI checks, build with `cargo build --locked --release` and run `./scripts/windows-smoke.ps1 -Executable target/release/nxmd.exe` in an interactive PowerShell 7 desktop session. The release workflow uploads the resulting screenshots alongside the source, recovery, and performance check results.

| Reproduction on Sprint 3 | Cause and verification |
| --- | --- |
| Select a long indented list item across several visual lines | The selection painter extended interior wraps to the editor margins. The painter now uses shaped glyph positions and the indentation of each line. The Windows test measures the highlight against glyph ink and compares the clipboard and cut bytes with independent expectations. |
| Select across a pipe table with short numbers and long wrapped text | A single row rectangle included unrelated cell padding. Selection now uses each cell’s source map, wrap positions and text alignment. The tall-table test checks the digits and the empty space beside them independently. |
| Scroll a tall grid until the header is above the viewport | Culling the header suppressed the frame around the entire table. The frame now uses the full table bounds. The Windows test measures all four column boundaries and observes movement of the row text. |
| Use the arrow keys next to a projected list marker or copy a partial formatted span | Hidden prefixes could be skipped, and copy could expand the selected source. Marker navigation now exposes the source, and clipboard and cut share the precise byte range. |
| Run Cut while another process holds the Windows clipboard | The GPUI write returned without reporting failure. nxmd now retries brief contention and requires a successful Unicode text write before removing the source. The native test holds the clipboard open through the failure and verifies the selection can still be copied and cut. |

Enter inserts a source newline and continues a list item. Shift+Enter uses a portable two-space break and a hanging continuation inside an item. Within a fence both keys insert a literal newline. Inside a table the keys navigate down and up, preserving the pipe structure. These operations retain the local line-ending convention. The code language label shows a dropdown indicator only when a picker is installed.

## Local Windows diagnostics

Each launch creates a file under `%LOCALAPPDATA%\nxmd\logs`. At startup nxmd inspects regular files directly in this directory for its diagnostic filename and historical startup header. Active or inaccessible processes, unrecognized files and links are left alone. Among eligible logs nxmd retains the newest within 10 files, 14 days and 10 MiB. Concurrent active processes or files Windows refuses to delete can temporarily exceed these limits. Pruning runs again on the next launch. Recovery checkpoints and transaction directories have independent lifecycles.

Routine records stop at 896 KiB per process. Panic reporting reserves up to 96 KiB on a preopened handle. Ordinary messages use buffered operating-system writes without a synchronous disk flush. The Windows exception filter writes a fixed-size exception code through its preopened handle. Production logs record startup, warning/error locations and panic backtraces. The GUI test harness enables additional operation details using `NXMD_DIAGNOSTICS=verbose`; use this only when troubleshooting with suitable test data. Diagnostics stay on the machine.
