//! Move a note in one filesystem operation. Never unlink a pathname on retry.
use anyhow::{Context, Result, bail};
use std::{fs, io::Read, path::Path};

pub(crate) fn move_note(source: &Path, dest: &Path, baseline: Option<&[u8]>) -> Result<()> {
    transact(source, dest, baseline, || Ok(()))
}

fn transact(
    source: &Path,
    dest: &Path,
    baseline: Option<&[u8]>,
    before_rename: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{Foundation::GENERIC_READ, Storage::FileSystem::*};
        // This handle is both the validation target and the rename target.
        // Refuse existing writers/deleters and exclude new ones until it closes.
        options
            .access_mode(GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(source).context("Cannot open note for move")?;
    if !file.metadata()?.is_file() || fs::symlink_metadata(source)?.file_type().is_symlink() {
        bail!("Only regular notes can be moved");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            bail!("Cannot move a reparse point");
        }
    }
    if let Some(expected) = baseline {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if bytes != expected {
            bail!("External change detected before move");
        }
    }
    before_rename()?;
    #[cfg(windows)]
    return rename_handle(&file, dest).context("Cannot move note without replacing destination");
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        renameat_with(CWD, source, CWD, dest, RenameFlags::NOREPLACE)
            .context("Cannot move note without replacing destination")?;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux", target_vendor = "apple")))]
    bail!("This platform does not provide a supported no-overwrite rename")
}

#[cfg(windows)]
fn rename_handle(file: &fs::File, dest: &Path) -> Result<()> {
    use std::{
        mem::{offset_of, size_of, size_of_val},
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };

    let name: Vec<u16> = dest.as_os_str().encode_wide().collect();
    let bytes = u32::try_from(size_of_val(name.as_slice()))?;
    let size = (offset_of!(FILE_RENAME_INFO, FileName) + bytes as usize)
        .max(size_of::<FILE_RENAME_INFO>());
    let len = u32::try_from(size)?;
    // usize storage aligns the pointer-sized RootDirectory field. The zeroed
    // header means ReplaceIfExists=false and RootDirectory=NULL (absolute name).
    let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).FileNameLength = bytes;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (&raw mut (*info).FileName).cast::<u16>(),
            name.len(),
        );
        if SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfo, info.cast(), len) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervening_publication_preserves_versions() -> Result<()> {
        let d = tempfile::tempdir()?;
        let (src, dst, stage) = (
            d.path().join("source.md"),
            d.path().join("moved.md"),
            d.path().join("external.tmp"),
        );
        fs::write(&src, b"editor\r\n")?;
        fs::write(&stage, b"external\n")?;
        // A competing atomic publication wins before validation.
        fs::rename(&stage, &src)?;
        assert!(move_note(&src, &dst, Some(b"editor\r\n")).is_err());
        assert_eq!(fs::read(&src)?, b"external\n");
        assert!(!dst.exists());
        // Destination appears after validation, before the kernel operation.
        assert!(
            transact(&src, &dst, Some(b"external\n"), || {
                fs::write(&dst, b"other destination")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&src)?, b"external\n");
        assert_eq!(fs::read(&dst)?, b"other destination");
        let retry = d.path().join("retry.md");
        move_note(&src, &retry, Some(b"external\n"))?;
        assert_eq!(fs::read(retry)?, b"external\n");
        assert!(!src.exists());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn competing_replace_during_validation_and_sharing_retry() -> Result<()> {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir()?;
        let (src, dst, stage) = (
            d.path().join("source.md"),
            d.path().join("moved.md"),
            d.path().join("stage"),
        );
        fs::write(&src, "original")?;
        fs::write(&stage, "newest")?;
        let blocker = fs::OpenOptions::new().read(true).share_mode(1).open(&src)?;
        let err = move_note(&src, &dst, Some(b"original")).unwrap_err();
        assert_eq!(
            err.downcast_ref::<std::io::Error>()
                .and_then(|e| e.raw_os_error()),
            Some(32)
        );
        assert!(!dst.exists());
        assert_eq!(fs::read(&src)?, b"original");
        drop(blocker);
        transact(&src, &dst, Some(b"original"), || {
            assert_eq!(
                fs::rename(&stage, &src).unwrap_err().raw_os_error(),
                Some(32)
            );
            Ok(())
        })?;
        assert!(!src.exists());
        fs::rename(&stage, &src)?;
        assert_eq!(fs::read(&src)?, b"newest");
        assert_eq!(fs::read(&dst)?, b"original");
        Ok(())
    }
}
