Based on zorite-editor 0.11.3 from crates.io (MIT; LICENSE retained).
Local change: normalize_loaded returns source unchanged. Upstream normalizes
words-attached $$ fences on load, which violates nxmd source preservation.
Review this patch and upstream normalization behavior before upgrading.

Sprint 1 image integration (nxmd):
- `image_embed.rs` recognizes standalone PNG/JPEG/GIF/WebP `![[path]]` images
  without allocating or modifying their source. `image_row` also reuses this
  recognition for list-item images. Non-image embeds retain upstream behavior.
- Image widgets reuse the existing block-image provider, projection and source
  mapping. Wiki-image source is shown on caret/selection, and resize grips are
  disabled so the upstream Markdown width writer cannot rewrite wiki syntax.
- `refresh_images` invalidates presentation after asynchronous loading without
  changing content or undo history.
- Parser regression tests include the exact pure source module in
  `tests/images.rs`; native GUI checks cover raster painting and source undo.
- Inline (mixed-prose) wiki embeds and `|width` sizing are not implemented.

Sprint 2 Live Preview:
- Ordered list projection starts at the first source ordinal at each nesting
  level; nesting keeps decimal markers and the original period/parenthesis.
  Clipboard copy preserves source ordinals rather than renumbering them.
- Formatting markers reveal on the active construct or selection. Active
  formatting uses source-grapheme deletion instead of rich-text marker deletion.
- Triple asterisks project combined bold/italic; inline code uses the mono font.
- The root editor_projection test target compiles the exact vendored scanner
  and its regression suite, without enabling GPUI's separate test platform.
- Table cells share inline-only projected text, styled runs and byte maps for
  measurement, paint, pointer hits, caret placement and row height. Caches are
  bounded and account for selection, caret reveal, font size and viewport width.
- Column sizing uses measured preferred and longest-word widths. Flexible columns
  share available width; genuinely wide tables retain horizontal scrolling. Both
  edge gutters reserve room for controls. Wrapped alignment is shared by paint
  and caret/hit geometry; vertical movement visits wrapped cell lines and never
  lands inside a UTF-8 character.
- Nested asterisk delimiters match runs while skipping escaped punctuation and
  inline code. Arrow navigation determines active syntax from current source,
  avoiding stale-map skips when multiple events arrive before a frame.
- Windows editor smoke tests exercise actual keyboard/clipboard input, source
  edits, undo/redo, tab selection retention and table edits after window resizing.
