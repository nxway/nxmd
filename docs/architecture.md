# Foundation decision — 2026-11-11

Inspected current upstream sources before implementing the editor integration:

* [Markion](https://github.com/willmove/markion), HEAD ea54556dcfc11cc1a6a86cca84d2807cf2086ea6, MIT. Its current Cargo manifest is version 0.5.2. Windows release pipeline and visual projection/editing code exist, including a vendored Windows international-input fix. It is a substantial application, with AI, Git sync, publishing, import/export and updater dependencies. Adapting the whole application would import considerable unwanted scope; extracting its tightly coupled editor would require extensive maintenance.
* [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), Apache-2.0. Native Windows backend exists. Selected the published `gpui-pre` 0.3 family used by Zorite, unified through Cargo.lock. This is a changing API, not a promise of stable framework compatibility. We do not depend on Zed's GPL application/editor crates.
* [gpui-component](https://github.com/longbridge/gpui-component), HEAD b7fe6ab3657069b2fc58e6093974aeaf0274e7f0, Apache-2.0 code. Active native cross-platform controls and sophisticated input/display-map code, but a larger toolkit and no reason to implement Markdown reveal ourselves when a focused component provides it.
* [zorite-editor](https://github.com/packetThrower/zorite), MIT, published 0.11.3 on 2026-10-29 (registry index). Inspected README, API and source. Reusable EntityInputHandler-based editor with source mapping, grapheme navigation, undo coalescing, IME, Markdown reveal, tables and host image hooks. This is the selected engine. A documented vendor patch disables source normalization on load, preserving math fences. Its upstream tests and claims do not substitute for our Windows acceptance testing.

## Boundaries

`vault`: filesystem containment, UTF-8 document baseline and atomic replacement.
`config`: session JSON in the user's configuration directory, outside the vault.
`attachments`: collision-safe imports and vault-local path resolution.
`ui`: GPUI shell, editor entities per tab, subscriptions and save scheduling.
`theme`: editor palette. `main`: native application lifecycle.

Markdown remains the editor's source text. We do not parse and serialize it through an AST on save. Each tab retains its own editor and undo history. The host does not reparse Markdown on edits; parsing/projection belongs to the reused engine. This engine uses a String, not a rope, and its auto-growing layout is not yet a verified large-document solution. Do not claim incremental or bounded-cost editing without profiling it.

No HTTP client is initialized and no remote image loader is installed by nxmd. Framework dependencies may contain networking support; nxmd does not use it.

## Safety limits

Writes use same-directory temporary files, flush data and compare original bytes twice before atomic replacement. This detects external edits but is not a filesystem transaction with noncooperating writers: there remains a race between the last comparison and replacement. Symlink containment likewise is not hardened against adversarial concurrent directory replacement. Do not describe either as race-free. Renames use hard-link/unlink and refuse existing destinations; unsupported filesystems produce errors. Deletions retain notes under `.nxmd-trash`.

Closing the window attempts dirty saves and refuses closure on failure. Process termination/crashes can still lose edits within the debounce interval; durable draft recovery is not implemented. External changes are detected by polling open notes; there is no recursive watcher. Session errors appear in the status bar.
