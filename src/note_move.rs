//! Move a note in one filesystem operation. Never unlink a pathname on retry.
use anyhow::{Context, Result, bail};
use std::{fs, io::Read, path::Path};

pub(crate) fn move_note(
    source: &Path,
    dest: &Path,
    expected: Option<(&fs::File, &[u8])>,
) -> Result<()> {
    transact(source, dest, expected, || Ok(()))
}

fn transact(
    source: &Path,
    dest: &Path,
    expected: Option<(&fs::File, &[u8])>,
    before_rename: impl FnOnce() -> Result<()>,
) -> Result<()> {
    #[cfg(windows)]
    {
        crate::windows_save::check_pending(source)?;
        crate::windows_save::check_pending(dest)?;
    }
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
    if let Some((original, expected)) = expected {
        #[cfg(any(unix, windows))]
        if identity(original)? != identity(&file)? {
            bail!("Source file was replaced while choosing the destination");
        }
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
        // Unix cannot lock out pathname replacement. Refuse a replacement
        // already visible here. The single rename still preserves a version
        // published during the final syscall race, under its new name.
        if identity(&fs::File::open(source)?)? != identity(&file)? {
            bail!("Source file was replaced before rename");
        }
        renameat_with(CWD, source, CWD, dest, RenameFlags::NOREPLACE)
            .context("Cannot move note without replacing destination")?;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "linux", target_vendor = "apple")))]
    bail!("This platform does not provide a supported no-overwrite rename")
}

// Both handles stay live through comparison, preventing reuse of an old file ID.
#[cfg(windows)]
fn identity(file: &fs::File) -> Result<(u64, [u8; 16])> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut id = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&raw mut id).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("Cannot verify move source identity");
    }
    Ok((id.VolumeSerialNumber, id.FileId.Identifier))
}

#[cfg(unix)]
fn identity(file: &fs::File) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let info = file.metadata()?;
    Ok((info.dev(), info.ino()))
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
        let original = fs::File::open(&src)?;
        // A competing atomic publication wins before validation.
        fs::rename(&stage, &src)?;
        assert!(move_note(&src, &dst, Some((&original, b"editor\r\n"))).is_err());
        assert_eq!(fs::read(&src)?, b"external\n");
        assert!(!dst.exists());
        let current = fs::File::open(&src)?;
        // Destination appears after validation, before the kernel operation.
        assert!(
            transact(&src, &dst, Some((&current, b"external\n")), || {
                fs::write(&dst, b"other destination")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&src)?, b"external\n");
        assert_eq!(fs::read(&dst)?, b"other destination");
        let retry = d.path().join("retry.md");
        move_note(&src, &retry, Some((&current, b"external\n")))?;
        assert_eq!(fs::read(retry)?, b"external\n");
        assert!(!src.exists());
        Ok(())
    }

    #[test]
    fn replacement_with_identical_bytes_is_a_different_note() -> Result<()> {
        let d = tempfile::tempdir()?;
        let (src, dst, stage) = (
            d.path().join("a.md"),
            d.path().join("b.md"),
            d.path().join("stage"),
        );
        fs::write(&src, "same bytes")?;
        let original = fs::File::open(&src)?;
        fs::write(&stage, "same bytes")?;
        fs::rename(stage, &src)?;
        assert!(move_note(&src, &dst, Some((&original, b"same bytes"))).is_err());
        assert_eq!(fs::read(src)?, b"same bytes");
        assert!(!dst.exists());
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
        let original = fs::File::open(&src)?;
        let blocker = fs::OpenOptions::new().read(true).share_mode(1).open(&src)?;
        let err = move_note(&src, &dst, Some((&original, b"original"))).unwrap_err();
        assert_eq!(
            err.downcast_ref::<std::io::Error>()
                .and_then(|e| e.raw_os_error()),
            Some(32)
        );
        assert!(!dst.exists());
        assert_eq!(fs::read(&src)?, b"original");
        drop(blocker);
        transact(&src, &dst, Some((&original, b"original")), || {
            // The source open above reports ERROR_SHARING_VIOLATION. A
            // competing rename can instead report ERROR_ACCESS_DENIED.
            let error = fs::rename(&stage, &src).unwrap_err();
            assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
            assert_eq!(fs::read(&src)?, b"original");
            assert_eq!(fs::read(&stage)?, b"newest");
            assert!(!dst.exists());
            Ok(())
        })?;
        assert!(!src.exists());
        fs::rename(&stage, &src)?;
        assert_eq!(fs::read(&src)?, b"newest");
        assert_eq!(fs::read(&dst)?, b"original");
        Ok(())
    }
}
