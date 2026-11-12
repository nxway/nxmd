# Images and explorer integration

PNG, JPEG, GIF and WebP files appear in their real vault directories with an image
icon, folders-first natural sorting, existing truncation and virtualized rows.
Click an image to open a read-only fit-to-window tab. Small images are not enlarged;
aspect ratio is preserved. Image paths restore with the session. Markdown tabs keep
their editor entities and undo history. Ctrl+S never writes an image tab.

Paste a Windows bitmap into a Markdown tab, or drop up to 16 image files. Imports
run off the UI thread, validate data before publishing, allocate collision-safe
`attachments/image-N.ext` files without overwrite, and insert portable relative
Markdown references. Invalid batch entries report errors; successful entries still
insert. Imports into a closed note remain real files and report that insertion was
skipped. Pasting while another batch is running reports that it is busy. Image paste
currently inserts at the caret (it does not replace selected text). No remote images
are fetched, and vault metadata is excluded from discovery and attachment resolution.

## Synchronization and lookup

A `notify` 8 native watcher runs for the active vault (ReadDirectoryChangesW on
Windows, inotify on Linux). Only safe, non-hidden, non-reparse directories are
watched, individually and non-recursively. A worker scans filename metadata once to
build the basename index; root rows publish before that index completes. There is no
image decoding during discovery. Events are batched for 150 ms, then affected parent
directories are reconciled; new directory subtrees are indexed, deleted subtrees and
watches are removed. Expanded UI rows are rebuilt from cached snapshots without disk
access, preserving expansion and scroll position. Internal operations request an
immediate queued reconciliation. F5 is a manual recovery action. Queue overflow or
watcher errors trigger background recovery scans; watcher errors remain visible.
The watcher and worker stop and release their handles when changing vaults.

Image-only `![[image.png]]`, `![[Pictures/image.png]]` and nested-path block embeds
use Zorite's existing image projection. Moving the caret/selection into a wiki image
shows editable original source. Existing Markdown, BOMs and CRLF are not serialized
or normalized by image lookup. Resolution tries a valid note-relative path, then a
vault-root-relative path, then a **unique** indexed basename (case-insensitive).
Ambiguous names show an error instead of choosing a file. Traversal may move up from
a nested note but cannot leave the vault; absolute/drive/ADS paths, hidden metadata,
unsafe Windows aliases and escaping symlink/junction targets are rejected.

## Resource limits

Decoding runs one request at a time on the GPUI background executor. Encoded data is
limited to 32 MiB, dimensions to 8192 per edge and 16 megapixels, and decoder
allocation requests to 64 MiB. Display rasters are reduced to at most 2048 per edge;
original attachment bytes remain untouched. GIF and animated WebP use a first frame.

The shared decoded cache admits at most 32 MiB of distinct raster pixels and 128
reference entries, shares canonical-path aliases between notes and image tabs, and
evicts inactive owners first. A single note exceeding that pixel budget displays a
clear limit message rather than cycling through endless eviction/redecode. Close,
F5 and relevant image/directory changes release or invalidate cached resources.
Encoded input, a currently decoding source, resized output and GPUI/GPU allocations
are additional transient/framework costs; 32 MiB is not a total-process RAM promise.

## Deliberate limits

- No animation, zoom, editing, thumbnails or attachment-location settings.
- No mixed-prose inline wiki image rendering or `|300` / `|300x200` sizing. Standard
  inline Markdown images retain the editor's existing behavior.
- No automatic link rewriting on file moves. Move related files deliberately;
  nxmd never silently rewrites other notes.
- Image context menus open the viewer; binary file rename/copy/trash controls are
  not added. Existing Markdown file operations remain available.
- Filesystem containment and optimistic saves retain the documented races against
  adversarial concurrent filesystem replacement; they are not OS transactions.

## Validation and measurement

`cargo test --locked --no-default-features` covers formats, filenames, import safety,
path resolution, ambiguity, native watcher changes/lifecycle and source preservation.
`cargo test --locked --release --bin nxmd` tests actual cache limits and alias reuse.
The Windows workflow retains the existing dialog/editor safety smoke and adds native
bitmap paste, image painting, explorer image opening, source edit/undo, read-only
save, image session restoration, external changes and a 1500-image directory.
Actual run outcomes and artifacts, rather than this test inventory, establish which
checks passed for a given commit.

`windows-performance.ps1` compares the last UI release (run 43151664774, commit
59e2647) and the sprint on the same runner with the same 250-note workload. It records
three alternating samples of working set, private bytes, five-second idle CPU time
and top-level-window appearance (not cold/rendered startup). The image smoke records
`image-performance.json`: bitmap paste through viewer display including autosave/UI
navigation, cached tree expansion handler time (not full-frame latency), opening 12
1024-square images through the UI, and process memory before/after. The image workload
has no comparable old viewer, so those are absolute measurements, not speedup claims.
Windows shell drag-and-drop and comprehensive IME acceptance remain separate manual
checks unless explicitly exercised and reported by a native test.
