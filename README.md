# nxmd

Native Rust Markdown editor using GPUI and zorite-editor. Local Markdown files are the storage format. No browser UI, account or cloud service.

## Windows build

Install Rust stable with the `x86_64-pc-windows-msvc` toolchain, Visual Studio 2022 C++ Build Tools and Windows SDK. From this directory:

```powershell
cargo test --locked --no-default-features
cargo build --locked --release
.\target\release\nxmd.exe C:\Notes
```

The vault argument is optional; the previous session is restored otherwise. Ctrl+O opens a directory, Ctrl+N creates a note, Ctrl+S saves. Ctrl+W closes a tab; Ctrl+Shift+L toggles the sidebar. Standard editing shortcuts are supplied by zorite-editor. Right-click a file for rename/move, save as copy or recoverable deletion; these actions target the clicked file. Close tabs with their × button, middle-click or Ctrl+W. Drag the explorer divider to resize it; width and visibility are restored with the session. Save-as-copy refuses to overwrite existing files.

## Implemented integration

* Charcoal/sage native GPUI shell, embedded vector icons, compact virtualized explorer, Unicode-aware natural sorting (folders first), tooltips and horizontally scrollable tabs.
* Reused live Markdown editor: heading styles, inline emphasis/code, lists, tasks, quotes, rules, links and tables with caret-aware source reveal. Windows smoke coverage includes typing, selection, undo/redo and source preservation.
* Debounced autosave, explicit save, UTF-8 loading, source-text persistence, external-change conflict detection and close prevention on failed dirty saves.
* Recent vault history and tab paths persisted outside the vault; previous vault/tabs restore.
* Local image provider, PNG/JPEG/GIF/WebP import, image-clipboard handler and file-drop handler. Images are copied to `attachments`; names use exclusive creation. GIF/WebP rendering currently decodes the first frame. Path resolution supports note-relative paths and vault-root fallback. These UI integrations require runtime verification.
* Folder creation, sidebar toggle and safe tab closing.
* Non-overwriting note move/rename; note deletion retains a recovery copy in `.nxmd-trash`.

## Still incomplete

Comprehensive IME and clipboard testing; folder rename/move/delete controls; tab reordering; recent-vault picker; settings UI and configurable attachments location; conflict reload/merge; recovery after crashes; file-tree watcher; follow-link actions; explicit Obsidian embed-provider integration; asynchronous image decoding/bounded cache; virtualized large-document rendering. Image paste inserts at the caret rather than replacing a selection. Basic external-file-drop semantics need native OS testing.

This is an initial implementation, not a completed production release. See [foundation and safety decisions](docs/architecture.md). The Windows workflow records comparable process measurements; methodology and limitations are in the validation record.

## Tests

```sh
cargo test --locked --no-default-features
cargo check --locked
```

Core tests exercise byte preservation, external-conflict refusal, containment, collisions and relative image paths. They do not establish end-to-end editor correctness. Windows CI builds the locked release, audits PE dependencies, publishes the executable, and drives the real window through native-dialog, context-menu, tab, conflict and panic-log checks. See the observed run links in the validation record.

See [measured validation and remaining Windows acceptance](docs/validation.md).
