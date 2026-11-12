//! Vault boundaries and optimistic, atomic persistence. No Markdown reserialization.
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
pub struct Vault {
    pub root: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}
pub struct Document {
    pub path: PathBuf,
    pub text: String,
    baseline: Vec<u8>,
}
impl Vault {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = dunce::canonicalize(root.as_ref())?;
        if !root.is_dir() {
            bail!("Vault must be a directory");
        }
        Ok(Self { root })
    }
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf> {
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("Path must stay inside the vault");
        }
        if relative.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".obsidian")
        }) {
            bail!("Obsidian configuration is protected");
        }
        for component in relative.components() {
            if let Component::Normal(name) = component {
                let name = name.to_string_lossy();
                let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                    || ["COM", "LPT"].iter().any(|prefix| {
                        stem.strip_prefix(prefix).is_some_and(|n| {
                            matches!(
                                n,
                                "1" | "2"
                                    | "3"
                                    | "4"
                                    | "5"
                                    | "6"
                                    | "7"
                                    | "8"
                                    | "9"
                                    | "¹"
                                    | "²"
                                    | "³"
                            )
                        })
                    });
                if reserved
                    || name.chars().any(char::is_control)
                    || name.contains([':', '\\', '<', '>', '"', '|', '?', '*'])
                    || name.ends_with(['.', ' '])
                {
                    bail!("Unsupported Windows filename");
                }
            }
        }
        let path = self.root.join(relative);
        let mut parent = path.as_path();
        while !parent.exists() {
            parent = parent.parent().context("Invalid path")?;
        }
        let canonical_parent = dunce::canonicalize(parent)?;
        let inside = canonical_parent
            .strip_prefix(&self.root)
            .context("Symlink escapes vault")?;
        if inside.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(".obsidian")
        }) {
            bail!("Obsidian configuration is protected through filesystem aliases too");
        }
        Ok(path)
    }
    pub fn entries(&self, dir: &Path) -> Result<Vec<Entry>> {
        let directory_path = self.resolve(dir)?;
        let metadata = fs::symlink_metadata(&directory_path)?;
        if metadata.file_type().is_symlink() {
            bail!("Linked directories are not enumerated");
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                bail!("Reparse directories are not enumerated");
            }
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(directory_path)? {
            let entry = entry?;
            let name = entry.file_name();
            let kind = match entry.file_type() {
                Ok(k) => k,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if name.to_string_lossy().starts_with('.') || kind.is_symlink() {
                continue;
            }
            let directory = kind.is_dir();
            let path = entry.path().strip_prefix(&self.root)?.to_path_buf();
            // Never enumerate reparse points (including Windows junctions), hidden
            // metadata, or aliases that cannot pass the normal vault boundary.
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if entry.metadata()?.file_attributes() & (0x400 | 0x2 | 0x4) != 0 {
                    continue;
                }
            }
            if self.resolve(&path).is_err() {
                continue;
            }
            if directory
                || crate::images::supported(&path)
                || path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                out.push(Entry { path, directory });
            }
        }
        out.sort_by_cached_key(|e| {
            (
                !e.directory,
                crate::natural::NaturalKey::new(
                    &e.path.file_name().unwrap_or_default().to_string_lossy(),
                ),
            )
        });
        Ok(out)
    }
    pub fn create(&self, path: &Path, directory: bool) -> Result<()> {
        let path = self.resolve(path)?;
        if directory {
            fs::create_dir(path)?;
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .sync_all()?;
        }
        Ok(())
    }
    pub fn load(&self, path: &Path) -> Result<Document> {
        let baseline = fs::read(self.resolve(path)?)?;
        let text = String::from_utf8(baseline.clone()).context("Note is not UTF-8")?;
        Ok(Document {
            path: path.into(),
            text,
            baseline,
        })
    }
    pub fn save(&self, doc: &mut Document) -> Result<()> {
        let path = self.resolve(&doc.path)?;
        if fs::read(&path)? != doc.baseline {
            bail!(
                "External change detected. Your edits remain in memory; save a copy before reloading."
            );
        }
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("Missing parent")?)?;
        temp.write_all(doc.text.as_bytes())?;
        temp.as_file().sync_all()?;
        // Recheck after preparing the replacement. Non-cooperating writers can still race.
        if fs::read(&path)? != doc.baseline {
            bail!("External change detected during save");
        }
        temp.persist(&path).map_err(|e| e.error)?;
        doc.baseline = doc.text.as_bytes().to_vec();
        Ok(())
    }
    /// Move into a newly allocated recovery directory; no destructive deletion.
    pub fn trash(&self, relative: &Path) -> Result<PathBuf> {
        let source = self.resolve(relative)?;
        if source == self.root {
            bail!("Cannot delete the vault root");
        }
        let trash = self.resolve(Path::new(".nxmd-trash"))?;
        fs::create_dir_all(&trash)?;
        let slot = tempfile::Builder::new()
            .prefix("deleted-")
            .tempdir_in(trash)?
            .keep();
        let dest = slot.join(source.file_name().context("Missing filename")?);
        fs::rename(source, &dest)?;
        Ok(dest)
    }
    /// A hard-link followed by unlink provides no-clobber file moves on the same volume.
    pub fn move_note(&self, from: &Path, to: &Path) -> Result<()> {
        let source = self.resolve(from)?;
        let dest = self.resolve(to)?;
        if !source.is_file() {
            bail!("Only notes can be moved with this operation");
        }
        fs::hard_link(&source, &dest)
            .context("Destination exists or filesystem does not support safe moves")?;
        fs::remove_file(source)?;
        Ok(())
    }
    pub fn changed(&self, doc: &Document) -> bool {
        self.resolve(&doc.path)
            .and_then(|p| Ok(fs::read(p)?))
            .map_or(true, |b| b != doc.baseline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_markdown_bytes() -> Result<()> {
        let d = tempfile::tempdir()?;
        let v = Vault::open(d.path())?;
        let raw = "\u{feff}# 日本語\r\n\r\n**bold**  \r\n![[photo.png]]\r\n";
        fs::write(d.path().join("a.md"), raw)?;
        let mut doc = v.load(Path::new("a.md"))?;
        v.save(&mut doc)?;
        assert_eq!(fs::read(d.path().join("a.md"))?, raw.as_bytes());
        Ok(())
    }
    #[test]
    fn conflict_preserves_both_versions() -> Result<()> {
        let d = tempfile::tempdir()?;
        let v = Vault::open(d.path())?;
        v.create(Path::new("a.md"), false)?;
        let mut doc = v.load(Path::new("a.md"))?;
        doc.text = "mine".into();
        fs::write(d.path().join("a.md"), "theirs")?;
        assert!(v.save(&mut doc).is_err());
        assert_eq!(doc.text, "mine");
        assert_eq!(fs::read_to_string(d.path().join("a.md"))?, "theirs");
        Ok(())
    }
    #[test]
    fn boundary_and_collision() -> Result<()> {
        let d = tempfile::tempdir()?;
        let v = Vault::open(d.path())?;
        assert!(v.resolve(Path::new("../escape")).is_err());
        assert!(v.resolve(Path::new(".obsidian/config")).is_err());
        v.create(Path::new("a.md"), false)?;
        assert!(v.create(Path::new("a.md"), false).is_err());
        Ok(())
    }
}
