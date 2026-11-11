use crate::vault::Vault;
use anyhow::{Result, bail};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
/// Unique create_new files: an existing attachment is never replaced.
pub fn import(vault: &Vault, note: &Path, bytes: &[u8]) -> Result<String> {
    if bytes.len() > 32 * 1024 * 1024 {
        bail!("Image exceeds 32 MiB");
    }
    let format = image::guess_format(bytes)?;
    let ext = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        _ => bail!("Unsupported image format"),
    };
    let dir = vault.resolve(Path::new("attachments"))?;
    std::fs::create_dir_all(&dir)?;
    for n in 1..100000 {
        let name = format!("image-{n}.{ext}");
        let target = dir.join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                    let _ = std::fs::remove_file(target);
                    return Err(e.into());
                }
                let prefix =
                    "../".repeat(note.parent().unwrap_or(Path::new("")).components().count());
                return Ok(format!("![]({prefix}attachments/{name})"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("Attachment name space exhausted")
}
pub fn resolve(v: &Vault, note: &Path, src: &str) -> Result<PathBuf> {
    let src = percent_encoding::percent_decode_str(src).decode_utf8()?;
    if src.contains(":") || src.starts_with('/') || src.contains('\\') {
        bail!("Only vault-local images are supported");
    }
    let base = v.root.join(note.parent().unwrap_or(Path::new("")));
    for p in [base.join(src.as_ref()), v.root.join(src.as_ref())] {
        if let Ok(p) = dunce::canonicalize(&p) {
            if p.starts_with(&v.root) {
                v.resolve(p.strip_prefix(&v.root)?)?;
                return Ok(p);
            }
        }
    }
    bail!("Image not found inside vault")
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
        assert_eq!(
            resolve(&v, Path::new("nested/n.md"), "../a%20b.png")?,
            d.path().join("a b.png")
        );
        assert!(resolve(&v, Path::new("n.md"), "https://example.com/a.png").is_err());
        Ok(())
    }
}
