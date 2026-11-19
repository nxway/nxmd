//! Windows replacement keeps a uniquely owned transaction directory on any
//! ambiguous outcome. Never restore over a pathname another writer may own.
use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

pub(crate) fn journal(path: &Path) -> PathBuf {
    // Tie retries to this spelling of the target. The unique backup below
    // remains discoverable even if the caller later uses a different alias.
    path.with_file_name(format!(
        ".nxmd-save-{}",
        crate::recovery::fingerprint(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .as_bytes()
        )
    ))
}
pub(crate) fn check_pending(path: &Path) -> Result<()> {
    let dir = journal(path);
    if dir.try_exists()? {
        bail!(
            "Unresolved save transaction at {}. Save a separate copy and inspect the retained versions.",
            dir.display()
        )
    }
    Ok(())
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn replace(path: &Path, stage: &Path, backup: &Path) -> std::io::Result<()> {
    let (p, s, b) = (wide(path), wide(stage), wide(backup));
    // All three NUL-terminated buffers live until the synchronous call returns.
    if unsafe {
        ReplaceFileW(
            p.as_ptr(),
            s.as_ptr(),
            b.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn commit(
    path: &Path,
    temp: tempfile::NamedTempFile,
    baseline: &[u8],
    editor: &[u8],
) -> Result<()> {
    transact(path, temp, baseline, editor, replace)
}
fn transact(
    path: &Path,
    temp: tempfile::NamedTempFile,
    baseline: &[u8],
    editor: &[u8],
    replace: impl FnOnce(&Path, &Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    let dir = journal(path);
    fs::create_dir(&dir).with_context(|| {
        format!(
            "Cannot reserve save journal {}. Inspect any earlier transaction before retrying.",
            dir.display()
        )
    })?;
    let backup = dir.join("displaced");
    let stage = dir.join("editor");
    // Record the intended destination before moving either version.
    let mut manifest = fs::File::create_new(dir.join("target.json"))?;
    serde_json::to_writer(&mut manifest, path)?;
    manifest.flush()?;
    manifest.sync_all()?;
    drop(manifest);
    drop(temp.persist_noclobber(&stage)?);
    if let Err(e) = replace(path, &stage, &backup) {
        // A sharing violation with all three names in their pre-call state is
        // retryable after the other handle closes. Touch only our own scratch files.
        if e.raw_os_error() == Some(32)
            && matches!(backup.try_exists(), Ok(false))
            && fs::read(path).as_deref().ok() == Some(baseline)
            && fs::read(&stage).as_deref().ok() == Some(editor)
        {
            fs::remove_file(&stage)?;
            fs::remove_file(dir.join("target.json"))?;
            fs::remove_dir(&dir)?;
            bail!("File is in use. Close the other program and retry Save. {e}");
        }
        // 1175/1176 generally retain both original names; with 1177 the
        // original can already be at backup. Preserve all paths even for
        // unfamiliar errors. No automatic retry or TempPath cleanup here.
        bail!(
            "Save incomplete ({e}). Inspect {}. Original path present: {:?}, staged editor present: {:?}, displaced version present: {:?}.",
            dir.display(),
            path.try_exists(),
            stage.try_exists(),
            backup.try_exists()
        );
    }
    if fs::read(&backup).as_deref().ok() != Some(baseline)
        || fs::read(path).as_deref().ok() != Some(editor)
    {
        bail!(
            "Conflict after replacement. Inspect {} and the displaced version at {}. Editor recovery retained.",
            path.display(),
            backup.display()
        )
    }
    fs::remove_file(&backup)?;
    fs::remove_file(dir.join("target.json"))?;
    fs::remove_dir(&dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stage(root: &Path) -> Result<tempfile::NamedTempFile> {
        let mut t = tempfile::NamedTempFile::new_in(root)?;
        t.write_all(b"mine")?;
        t.as_file().sync_all()?;
        Ok(t)
    }
    #[test]
    fn competing_writer_at_commit_retains_displaced_version() -> Result<()> {
        let root = tempfile::tempdir()?;
        let p = root.path().join("note.md");
        fs::write(&p, b"base")?;
        let err = transact(&p, stage(root.path())?, b"base", b"mine", |p, s, b| {
            fs::write(p, b"external")?;
            replace(p, s, b)
        })
        .unwrap_err();
        assert!(err.to_string().contains("Conflict after replacement"));
        assert_eq!(fs::read(&p)?, b"mine");
        assert_eq!(fs::read(journal(&p).join("displaced"))?, b"external");
        Ok(())
    }
    #[test]
    fn partial_failures_retain_survivors() -> Result<()> {
        for code in [1175, 1176, 1177] {
            let root = tempfile::tempdir()?;
            let p = root.path().join("n.md");
            fs::write(&p, b"old")?;
            assert!(
                transact(&p, stage(root.path())?, b"old", b"mine", |p, _, b| {
                    if code == 1177 {
                        fs::rename(p, b)?;
                    }
                    Err(std::io::Error::from_raw_os_error(code))
                })
                .is_err()
            );
            let dir = journal(&p);
            assert_eq!(fs::read(dir.join("editor"))?, b"mine");
            assert_eq!(
                fs::read(if code == 1177 {
                    dir.join("displaced")
                } else {
                    p
                })?,
                b"old"
            );
        }
        Ok(())
    }
}
