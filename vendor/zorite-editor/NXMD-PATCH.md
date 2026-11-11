Based on zorite-editor 0.11.3 from crates.io (MIT; LICENSE retained).
Local change: normalize_loaded returns source unchanged. Upstream normalizes
words-attached $$ fences on load, which violates nxmd source preservation.
Review this patch and upstream normalization behavior before upgrading.
