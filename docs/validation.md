# Validation record

2026-11-11, Linux x86_64 managed environment, Rust 1.99.1, debug build.

* `cargo build --locked`: passed (GPUI/Zorite deprecation warnings).
* `cargo test --locked --no-default-features`: 8 tests passed.
* Native executable launched on Xvfb with Mesa lavapipe software Vulkan.
* Restored a real Markdown note into a tab. Inspected the rendered window:
  headings, hidden formatting markers, tasks, quotes, inline code and table grid.
* Sent real X keyboard events: appended text, Ctrl+S, verified file bytes;
  Ctrl+Z and Ctrl+S removed inserted text; Ctrl+Shift+Z and debounce restored it.
* Changed that file externally, typed more, and invoked Ctrl+S. Verified the
  external file remained unchanged. Status displayed the conflict.

These are Linux smoke tests, not Windows/IME certification or comprehensive
editing tests. Image clipboard/drop and native dialogs are not covered by them.
The earlier Windows dialog fix passed release run [43005240014](https://github.com/nxway/nxmd/actions/runs/43005240014). UI-overhaul verification is recorded below after its Windows run.

## Storage workload

Reproduce: `cargo run --locked --no-default-features --example storage_bench`.
A temporary directory is populated with 5,000 notes, then listed. A 1.2 MB UTF-8
note is loaded and saved after an edit. One debug run on this container:

| Operation | Wall time |
| --- | ---: |
| List 5,000 notes | 13.75 ms |
| Load 1,200,000 bytes | 1.29 ms |
| Conflict-checked replacement save | 5.95 ms |

This is a single warm-cache sample on an ephemeral filesystem, not a statistical
benchmark or Windows result. Native startup, typing latency, long-document
scrolling and idle RAM require controlled release-build measurement on Windows.
The software-rendered debug process was approximately 207 MiB RSS in one sample;
that includes software graphics overhead and is not evidence of a memory
improvement over another editor.

## Wider Windows acceptance still required

Build with MSVC, open an existing vault with nested Unicode paths, type with
Japanese/Chinese IME, exercise selection/undo across hidden syntax and tables,
paste a screenshot, drop PNG/JPEG/GIF/WebP files, test Obsidian embeds, interrupt
saves, change files externally, reopen the session, and measure large-note and
large-vault behavior. Compare unchanged Markdown bytes, especially CRLF, BOM,
reference definitions and math fences. Keep backups during pre-release testing.

## UI overhaul validation

Local formatting, compilation and core tests pass (14 tests on Linux). Clippy completes with existing style/deprecation warnings; the pinned editor still uses deprecated GPUI scrolling APIs. The native application was inspected on Xvfb using a mixed-Markdown test vault, including mouse closing an inactive tab and opening a file context menu.

The Windows smoke script now checks source-preserving typing, selection undo/redo, all six asynchronous native dialogs, context actions targeting a non-active file, inactive-tab mouse closing, middle-click closing, refusal to close a dirty externally modified document, normal shutdown and persistent panic logging. It captures the actual window in the validation artifact. These checks do not establish IME, image clipboard or exhaustive Markdown editing correctness.

`windows-performance.ps1` compares the pre-overhaul executable from run 43005240014 with the candidate on the same runner. It alternates three launches of each against an identical synthetic vault (250 notes, one approximately 4 KB document open), with isolated configuration, a two-second settling period and five-second idle CPU sample. It records working set and private bytes separately. Window-appearance time ends when Windows exposes a top-level handle; it is neither fully rendered startup time nor a cold-cache benchmark. Shared-runner graphics overhead and scheduling limit generalization. The user's 23.9 MB Task Manager observation is not this benchmark's baseline. The baseline artifact has 30-day retention; future comparisons require an available baseline run.
