use nxmd::{attachments, vault::Vault};
use std::{fs, path::Path};
#[test]
fn safe_move_and_recoverable_delete() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    fs::write(dir.path().join("one.md"), "# One")?;
    fs::write(dir.path().join("two.md"), "# Two")?;
    assert!(
        v.move_note(Path::new("one.md"), Path::new("two.md"))
            .is_err()
    );
    assert_eq!(fs::read_to_string(dir.path().join("two.md"))?, "# Two");
    v.move_note(Path::new("one.md"), Path::new("renamed.md"))?;
    assert!(!dir.path().join("one.md").exists());
    let recovery = v.trash(Path::new("renamed.md"))?;
    assert_eq!(fs::read_to_string(recovery)?, "# One");
    Ok(())
}
#[test]
fn image_import_does_not_overwrite() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    let mut data = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(1, 1).write_to(&mut data, image::ImageFormat::Png)?;
    let a = attachments::import(&v, Path::new("nested/n.md"), data.get_ref())?;
    let b = attachments::import(&v, Path::new("nested/n.md"), data.get_ref())?;
    assert_ne!(a, b);
    assert!(a.contains("../attachments/"));
    assert_eq!(fs::read_dir(dir.path().join("attachments"))?.count(), 2);
    Ok(())
}
#[test]
fn windows_aliases_are_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    for path in [".obsidian./config", "a.md:stream", "folder\\escape.md"] {
        assert!(v.resolve(Path::new(path)).is_err(), "{path}");
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlink_escape_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link"))?;
    assert!(v.create(Path::new("link/escape.md"), false).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_dialog_paths_match_vault_root() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    // std canonicalize uses the verbatim Windows prefix; dialogs generally don't.
    let verbatim = dir.path().canonicalize()?;
    let v = Vault::open(&verbatim)?;
    let dialog_destination = dunce::canonicalize(dir.path())?.join("new.md");
    let relative = dialog_destination.strip_prefix(&v.root)?;
    v.create(relative, false)?;
    let mut doc = v.load(relative)?;
    doc.text = "# 日本語\r\n".into();
    v.save(&mut doc)?;
    assert_eq!(fs::read_to_string(dialog_destination)?, doc.text);
    Ok(())
}
