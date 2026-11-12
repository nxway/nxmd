use crate::{images, vault::Vault};
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Component, Path, PathBuf},
};
pub type ImageIndex = BTreeMap<String, Vec<PathBuf>>;

/// Validate before writing. Publish with no-clobber atomic persistence; a failed
/// write drops its temporary file and never inserts a reference into the note.
pub fn import(vault: &Vault, note: &Path, bytes: &[u8]) -> Result<String> {
    vault.resolve(note)?;
    images::decode(bytes)?;
    let ext = match image::guess_format(bytes)? {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        _ => bail!("Unsupported image format"),
    };
    let dir = vault.resolve(Path::new("attachments"))?;
    std::fs::create_dir_all(&dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    for n in 1..100000 {
        let name = format!("image-{n}.{ext}");
        let target = vault.resolve(&Path::new("attachments").join(&name))?;
        match temp.persist_noclobber(&target) {
            Ok(_) => {
                let depth = note
                    .parent()
                    .unwrap_or(Path::new(""))
                    .components()
                    .filter(|c| matches!(c, Component::Normal(_)))
                    .count();
                return Ok(format!("![]({}attachments/{name})", "../".repeat(depth)));
            }
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temp = e.file,
            Err(e) => return Err(e.error.into()),
        }
    }
    bail!("Attachment name space exhausted")
}
fn normalize(base: &Path, src: &str) -> Result<PathBuf> {
    let mut out = base.to_path_buf();
    for part in src.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !out.pop() {
                    bail!("Image path escapes vault");
                }
            }
            p => {
                if p.starts_with('.')
                    || p.ends_with(['.', ' '])
                    || p.contains([':', '\\', '\0', '|'])
                {
                    bail!("Unsafe image path component");
                }
                out.push(p);
            }
        }
    }
    Ok(out)
}
pub fn resolve(v: &Vault, note: &Path, src: &str) -> Result<PathBuf> {
    resolve_indexed(v, note, src, &ImageIndex::new())
}
pub fn resolve_indexed(v: &Vault, note: &Path, src: &str, index: &ImageIndex) -> Result<PathBuf> {
    v.resolve(note)?;
    let src = percent_encoding::percent_decode_str(src).decode_utf8()?;
    if src.is_empty() || src.contains([':', '\\', '\0', '|']) || src.starts_with('/') {
        bail!("Only vault-local image paths are supported (embed sizing is not supported)");
    }
    if !images::supported(Path::new(src.as_ref())) {
        bail!("Unsupported image extension");
    }
    // An escaping relative path is an error, never retried as a different image.
    let relative = normalize(note.parent().unwrap_or(Path::new("")), &src)?;
    for p in [Some(relative), normalize(Path::new(""), &src).ok()]
        .into_iter()
        .flatten()
    {
        let p = v.resolve(&p)?;
        if p.is_file() {
            return dunce::canonicalize(p).context("Cannot resolve image");
        }
    }
    if !src.contains('/') {
        if let Some(matches) = index.get(&src.to_lowercase()) {
            if matches.len() != 1 {
                bail!(
                    "Ambiguous image filename: {} ({} matches)",
                    src,
                    matches.len()
                );
            }
            let p = v.resolve(&matches[0])?;
            if p.is_file() {
                return Ok(dunce::canonicalize(p)?);
            }
        }
    }
    bail!("Image not found inside vault: {src}")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_nested_and_rejects_escape() -> Result<()> {
        let d = tempfile::tempdir()?;
        let v = Vault::open(d.path())?;
        std::fs::create_dir(d.path().join("nested"))?;
        std::fs::write(d.path().join("a b.png"), b"test")?;
        std::fs::write(d.path().join("nested/a b.png"), b"wrong image")?;
        let resolved = resolve(&v, Path::new("nested/n.md"), "../a%20b.png")?;
        assert_eq!(
            dunce::canonicalize(&resolved)?,
            dunce::canonicalize(d.path().join("a b.png"))?
        );
        assert_eq!(std::fs::read(resolved)?, b"test");
        assert!(resolve(&v, Path::new("n.md"), "https://example.com/a.png").is_err());
        Ok(())
    }
}
