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
    for path in [".obsidian./config", "a.md:stream", "../escape.md"] {
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

#[cfg(windows)]
#[test]
fn windows_nested_paths_and_dot_normalization() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    v.create(Path::new("folder"), true)?;
    v.create(Path::new(r"folder\note.md"), false)?;
    for path in [r"folder\note.md", "folder/note.md", r".\folder\.\note.md"] {
        assert_eq!(
            dunce::canonicalize(v.resolve(Path::new(path))?)?,
            dunce::canonicalize(dir.path().join("folder/note.md"))?
        );
        assert_eq!(v.load(Path::new(path))?.text, "");
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_traversal_prefixes_and_protected_paths_are_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    for path in [
        r"..\escape.md",
        r"folder\..\escape.md",
        "folder/../escape.md",
        r"C:\escape.md",
        r"C:escape.md",
        r"\escape.md",
        r"\\server\share\escape.md",
        r"\\?\C:\escape.md",
        r"\\.\NUL",
        r"folder\a.md:stream",
        r".obsidian\config.json",
        r".OBSIDIAN\config.json",
        r"folder\.obsidian\config.json",
        r".\.obsidian\config.json",
        r".obsidian.\config.json",
        r".obsidian \config.json",
    ] {
        assert!(
            v.resolve(Path::new(path)).is_err(),
            "accepted unsafe path: {path}"
        );
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_junctions_cannot_escape_or_access_obsidian() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    fs::create_dir(dir.path().join(".obsidian"))?;
    for (name, target) in [
        ("outside", outside.path().to_path_buf()),
        ("protected", dir.path().join(".obsidian")),
    ] {
        // Directory junctions do not require the symbolic-link privilege.
        let link = dir.path().join(name);
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()?;
        assert!(
            output.status.success(),
            "junction creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = v.create(&Path::new(name).join("escape.md"), false);
        fs::remove_dir(link)?;
        assert!(result.is_err(), "accepted junction: {name}");
        assert!(!target.join("escape.md").exists());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_into_obsidian_is_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    fs::create_dir(dir.path().join(".obsidian"))?;
    std::os::unix::fs::symlink(dir.path().join(".obsidian"), dir.path().join("alias"))?;
    assert!(v.create(Path::new("alias/config.json"), false).is_err());
    assert!(!dir.path().join(".obsidian/config.json").exists());
    Ok(())
}

#[test]
fn explorer_folders_first_and_natural_names() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let v = Vault::open(dir.path())?;
    for folder in ["48. Millie", "5. St. Nora", "2. Folder"] {
        v.create(Path::new(folder), true)?;
    }
    for file in ["10.md", "2.md", "a.md", "A.md"] {
        // Case-insensitive filesystems cannot contain both a.md and A.md.
        if !dir.path().join(file).exists() {
            v.create(Path::new(file), false)?;
        }
    }
    let entries = v.entries(Path::new(""))?;
    assert_eq!(
        entries[..3]
            .iter()
            .map(|e| e.path.to_string_lossy())
            .collect::<Vec<_>>(),
        vec!["2. Folder", "5. St. Nora", "48. Millie"]
    );
    assert_eq!(entries[3].path, Path::new("2.md"));
    assert_eq!(entries[4].path, Path::new("10.md"));
    Ok(())
}
