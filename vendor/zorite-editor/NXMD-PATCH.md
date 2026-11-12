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
