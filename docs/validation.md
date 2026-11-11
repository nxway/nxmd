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
Windows CI is configured but has not run in this session.

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

## Windows acceptance still required

Build with MSVC, open an existing vault with nested Unicode paths, type with
Japanese/Chinese IME, exercise selection/undo across hidden syntax and tables,
paste a screenshot, drop PNG/JPEG/GIF/WebP files, test Obsidian embeds, interrupt
saves, change files externally, reopen the session, and measure large-note and
large-vault behavior. Compare unchanged Markdown bytes, especially CRLF, BOM,
reference definitions and math fences. Keep backups during pre-release testing.
