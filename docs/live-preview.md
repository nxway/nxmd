# Live Preview correctness (Sprint 2)

The source string remains authoritative. Loading, copying and saving do not
round-trip Markdown through a parser or serializer. The vendored Zorite scanner
produces styled runs and display-byte-to-source-byte maps; GPUI still owns native
text shaping, input, clipboard integration and GPU rendering.

## Fixes

- Ordered lists start at their first source ordinal, independently at each nesting
  level. Numbered child lists stay decimal and retain `.` or `)`. Copy preserves
  the actual ordinals, including leading zeros and nonsequential source values.
  Following items render consecutively, as in CommonMark; source digits are not
  rewritten. Blank lines allow loose lists; unindented prose ends a list.
- Active inline constructs and selections reveal Markdown syntax. Combined and
  nested asterisk emphasis skips escaped punctuation and literal inline code
  while matching delimiters. Backspace/Delete inside revealed syntax edit the
  source grapheme. Rapid arrow input no longer depends exclusively on the previous
  frame's hidden-marker map.
- Table cells use inline-only Markdown projection, with one set of text/style
  runs and byte maps shared by measurement, paint, hit testing and caret placement.
  Column sizing measures actual styled text and longest words, shares available
  width, and retains horizontal scrolling when minimum content widths cannot fit.
  As the viewport narrows, flexible cell padding contracts and short values receive
  room to stay on one line when possible. Automatic layout leaves Markdown intact.
  Dragging a column divider or double-clicking it to fit explicitly saves the
  table's widths in the existing `<!-- table:… cols=… -->` marker. That edit can
  be undone, and the saved widths apply on reopen. Explicit widths can require
  scrolling in a smaller window. Row height uses the same wrapping as paint. Caret and hit geometry account for
  alignment on each wrapped line. Vertical navigation visits wrapped cell lines
  and clamps cross-cell positions to UTF-8 boundaries.
- Table projection/height caches are bounded. Cache keys include relevant source,
  selection/reveal state, font size and available width. Ordinary files and image
  storage, conflict handling, native dialogs and the application layout are
  unchanged.

The implementation and narrow upstream deviations are documented in
[`vendor/zorite-editor/NXMD-PATCH.md`](../vendor/zorite-editor/NXMD-PATCH.md).

## Reproducible checks

```powershell
cargo test --locked --no-default-features
cargo test --locked --release --test editor_projection
cargo test --locked --release --bin nxmd
cargo build --locked --release
./scripts/windows-smoke.ps1 -Executable ./target/release/nxmd.exe
```

`editor_projection` includes the exact vendored scanner and sizing allocator,
without GPUI's separate test platform. It covers list starting values, nested and
adjacent formatting, literal code, escapes, inline-only table interpretation,
Unicode mapping, syntax reveal, table cell ranges and width allocation.

The storage suite saves and reopens LF and BOM/CRLF fixtures containing YAML,
nonsequential lists, tables, escaped delimiters, unsupported syntax and Obsidian
embeds. Both unchanged saves and isolated edits compare exact bytes.

`windows-editor-smoke.ps1` runs inside the existing full Windows smoke test. It
uses actual native keyboard and clipboard events and checks the copied source,
saved bytes and process survival. It tests formatted insertion/deletion,
multiblock selection, undo/redo, selection/history across tabs, Unicode paste and
composed-grapheme deletion, table editing after resizing, wrapped navigation and
BOM/CRLF save/reopen. It captures list/inline and wide/narrow table screenshots.
Existing dialog, filesystem conflict, image, explorer and tab-close checks remain
part of that workflow. Assertions use observed clipboard/file/session results;
input mutations are never retried to disguise dropped events.

## Measurement and remaining limits

`windows-performance.ps1` alternates three baseline/candidate launches for each
of two synthetic workloads: a short note and a document with 120 formatted
sections, nested lists and tables, inside a 250-note vault. It records process
working set, private bytes, window-handle appearance, five-second idle CPU and
five native input-to-copy acknowledgement round trips. The workflow requests the audited pre-remediation build for comparison when available. Raw samples are in `performance.json` in the
validation artifact. These are shared-runner measurements, not cold-cache startup
or isolated keystroke-to-photon latency; compare distributions and workloads,
not a single sample.

Full document virtualization is not implemented. Scanning and some table layout
work can still scale with document size. The scanner supports a focused Markdown
subset, not every CommonMark/Obsidian extension; tables require leading pipes.
Unsupported syntax is preserved in source. Complex bidirectional wrapped text,
physical dead-key layouts and IMEs beyond the tested Microsoft Japanese profile need further testing.
