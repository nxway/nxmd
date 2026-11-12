//! nxmd: recognize image-only Obsidian block embeds without rewriting source.
pub fn image_embed(line: &str) -> Option<&str> {
    let src = line.trim().strip_prefix("![[")?.strip_suffix("]]")?;
    if src.is_empty() || src.contains(['|', '\n', '\r', '[', ']']) {
        return None;
    }
    let ext = src.rsplit_once('.')?.1;
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
    .then_some(src)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_embeds_preserve_borrowed_source() {
        for line in [
            "![[image.png]]",
            "![[Pictures/a b.JPG]]",
            "![[folder/subfolder/日本語.webp]]",
            " ![[image.gif]]\r",
        ] {
            let before = line.to_owned();
            assert!(image_embed(line).is_some());
            assert_eq!(line, before);
        }
        for line in [
            "![[note.md]]",
            "![[image.png|300]]",
            "x ![[image.png]]",
            "![[image.png]] tail",
            "```![[image.png]]",
            "![[image\n.png]]",
        ] {
            assert!(image_embed(line).is_none(), "{line}");
        }
    }
}
