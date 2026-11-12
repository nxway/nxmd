use nxmd::{
    attachments::{self, ImageIndex},
    catalog::{Catalog, Snapshot},
    images,
    vault::Vault,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
#[path = "../vendor/zorite-editor/src/image_embed.rs"]
mod image_embed;
fn png() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(8, 4)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
fn wait(c: &Catalog, predicate: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let s = c.snapshot();
        if s.ready && predicate(&s) {
            return s;
        }
        assert!(
            Instant::now() < until,
            "watch timeout: ready={}, error={:?}, dirs={:?}",
            s.ready,
            s.error,
            s.directories.keys()
        );
        std::thread::sleep(Duration::from_millis(30));
    }
}
#[test]
fn explorer_images_unicode_natural_order_and_metadata() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    for dir in ["10 Folder", "2 Folder", ".obsidian", ".nxmd-trash"] {
        fs::create_dir(d.path().join(dir))?;
    }
    for name in [
        "10.png",
        "2.JPG",
        "3.jpeg",
        "4.GIF",
        "5.webp",
        "6.md",
        "日本語 space.png",
        "ignored.txt",
        ".hidden.png",
        "CON.png",
        "unsafe.md:stream",
    ] {
        // Reserved names cannot be created on Windows, so test rejection separately.
        if !cfg!(windows) || !["CON.png", "unsafe.md:stream"].contains(&name) {
            fs::write(d.path().join(name), b"not decoded by explorer")?;
        }
    }
    fs::write(d.path().join("2 Folder/nested.png"), png())?;
    let list: Vec<_> = v
        .entries(Path::new(""))?
        .into_iter()
        .map(|e| e.path)
        .collect();
    assert_eq!(
        list,
        [
            "2 Folder",
            "10 Folder",
            "2.JPG",
            "3.jpeg",
            "4.GIF",
            "5.webp",
            "6.md",
            "10.png",
            "日本語 space.png"
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        v.entries(Path::new("2 Folder"))?[0].path,
        Path::new("2 Folder/nested.png")
    );
    for bad in [
        "CON.png",
        "NUL .png",
        "CONIN$.png",
        "NUL",
        "COM1.jpg",
        "LPT¹.md",
        "a.md:stream",
        "a?b.png",
    ] {
        assert!(v.resolve(Path::new(bad)).is_err(), "{bad}");
    }
    Ok(())
}
#[test]
fn import_collision_nested_reference_and_failure_are_atomic() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    let note = Path::new("one/日本語/two/note.md");
    fs::create_dir_all(d.path().join(note.parent().unwrap()))?;
    let original = b"unchanged note";
    fs::write(d.path().join(note), original)?;
    assert!(attachments::import(&v, note, b"\x89PNG\r\n\x1a\ntruncated").is_err());
    assert!(!d.path().join("attachments").exists());
    let a = attachments::import(&v, note, &png())?;
    let b = attachments::import(&v, note, &png())?;
    assert_eq!(a, "![](../../../attachments/image-1.png)");
    assert_eq!(b, "![](../../../attachments/image-2.png)");
    let path = attachments::resolve(&v, note, "../../../attachments/image-1.png")?;
    assert_eq!(fs::read(path)?, png());
    assert_eq!(fs::read(d.path().join(note))?, original);
    assert_eq!(fs::read_dir(d.path().join("attachments"))?.count(), 2);
    assert!(attachments::import(&v, Path::new("../escape.md"), &png()).is_err());
    Ok(())
}
#[test]
fn explicit_paths_unique_basename_and_ambiguity() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    for dir in ["Notes/Pictures/sub", "Pictures/sub", "Other"] {
        fs::create_dir_all(d.path().join(dir))?;
    }
    for name in [
        "Notes/Pictures/sub/a b.png",
        "Pictures/sub/a b.png",
        "Other/日本語.png",
    ] {
        fs::write(d.path().join(name), png())?;
    }
    let c = Catalog::new(v.clone());
    let s = wait(&c, |_| true);
    let note = Path::new("Notes/n.md");
    assert_eq!(
        attachments::resolve_indexed(&v, note, "Pictures/sub/a%20b.png", &s.images)?,
        dunce::canonicalize(d.path().join("Notes/Pictures/sub/a b.png"))?
    );
    assert_eq!(
        attachments::resolve_indexed(&v, Path::new("n.md"), "Pictures/sub/a b.png", &s.images)?,
        dunce::canonicalize(d.path().join("Pictures/sub/a b.png"))?
    );
    assert_eq!(
        attachments::resolve_indexed(&v, note, "日本語.png", &s.images)?,
        dunce::canonicalize(d.path().join("Other/日本語.png"))?
    );
    assert!(
        attachments::resolve_indexed(&v, note, "a b.png", &s.images)
            .unwrap_err()
            .to_string()
            .contains("Ambiguous")
    );
    assert!(attachments::resolve_indexed(&v, note, "missing.png", &s.images).is_err());
    Ok(())
}
#[test]
fn malicious_image_references_never_leave_vault() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    for bad in [
        "../escape.png",
        "%2e%2e/escape.png",
        "/absolute.png",
        "C:relative.png",
        "C:/a.png",
        r"\\server\share\a.png",
        r"folder\a.png",
        "https://example.com/a.png",
        ".obsidian/a.png",
        ".obsidian/../a.png",
        ".OBSIDIAN/a.png",
        ".obsidian./a.png",
        "a.png:stream",
        "file:///a.png",
        "CON.png",
        "a.png|300",
        "a%00.png",
    ] {
        assert!(
            attachments::resolve_indexed(&v, Path::new("n.md"), bad, &ImageIndex::new()).is_err(),
            "{bad}"
        );
    }
    Ok(())
}
#[test]
fn watcher_tracks_import_create_rename_delete_and_content_changes() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    let c = Catalog::new(v.clone());
    wait(&c, |_| true);
    attachments::import(&v, Path::new("n.md"), &png())?;
    let imported = wait(&c, |s| s.images.contains_key("image-1.png"));
    assert!(
        imported
            .directories
            .get(Path::new(""))
            .unwrap()
            .iter()
            .any(|e| e.path == Path::new("attachments"))
    );
    fs::create_dir(d.path().join("External"))?;
    fs::write(d.path().join("External/a.png"), png())?;
    wait(&c, |s| s.images.contains_key("a.png"));
    fs::rename(d.path().join("External"), d.path().join("Renamed"))?;
    let renamed = wait(&c, |s| {
        s.images
            .get("a.png")
            .is_some_and(|p| p == &[PathBuf::from("Renamed/a.png")])
    });
    fs::write(d.path().join("Renamed/a.png"), b"changed")?;
    wait(&c, |s| s.image_revision > renamed.image_revision);
    fs::remove_dir_all(d.path().join("Renamed"))?;
    wait(&c, |s| {
        !s.images.contains_key("a.png") && !s.directories.contains_key(Path::new("Renamed"))
    });
    fs::write(d.path().join("external.md"), b"text")?;
    wait(&c, |s| {
        s.directories
            .get(Path::new(""))
            .unwrap()
            .iter()
            .any(|e| e.path == Path::new("external.md"))
    });
    fs::rename(d.path().join("external.md"), d.path().join("renamed.md"))?;
    wait(&c, |s| {
        s.directories
            .get(Path::new(""))
            .unwrap()
            .iter()
            .any(|e| e.path == Path::new("renamed.md"))
    });
    fs::remove_file(d.path().join("renamed.md"))?;
    wait(&c, |s| {
        !s.directories
            .get(Path::new(""))
            .unwrap()
            .iter()
            .any(|e| e.path == Path::new("renamed.md"))
    });
    Ok(())
}
#[test]
fn watcher_switch_does_not_retain_old_vault_or_metadata() -> anyhow::Result<()> {
    let a = tempfile::tempdir()?;
    let b = tempfile::tempdir()?;
    let c = Catalog::new(Vault::open(a.path())?);
    wait(&c, |_| true);
    drop(c);
    fs::create_dir(b.path().join(".obsidian"))?;
    fs::write(b.path().join(".obsidian/private.png"), png())?;
    let c = Catalog::new(Vault::open(b.path())?);
    wait(&c, |_| true);
    fs::write(a.path().join("old.png"), png())?;
    fs::write(b.path().join("new.png"), png())?;
    let s = wait(&c, |s| s.images.contains_key("new.png"));
    assert!(!s.images.contains_key("old.png"));
    assert!(!s.images.contains_key("private.png"));
    Ok(())
}
#[test]
fn bounded_decode_fit_and_unchanged_bytes() -> anyhow::Result<()> {
    let bytes = png();
    let copy = bytes.clone();
    let d = images::decode(&bytes)?;
    assert_eq!(d.original, (8, 4));
    assert_eq!(bytes, copy);
    assert_eq!(images::fit((8, 4), (100., 100.)), (8., 4.));
    assert_eq!(images::fit((800, 400), (200., 300.)), (200., 100.));
    assert_eq!(images::fit((400, 800), (300., 200.)), (100., 200.));
    assert!(images::decode(b"corrupt image").is_err());
    // Header-only oversized BMP is unsupported without attempting any allocation.
    assert!(images::decode(&vec![0; images::MAX_ENCODED + 1]).is_err());
    let mut big = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(8193, 1).write_to(&mut big, image::ImageFormat::Png)?;
    assert!(
        images::decode(big.get_ref())
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn linked_images_and_directories_cannot_escape() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    fs::write(outside.path().join("a.png"), png())?;
    std::os::unix::fs::symlink(outside.path(), d.path().join("link"))?;
    std::os::unix::fs::symlink(outside.path().join("a.png"), d.path().join("alias.png"))?;
    assert!(attachments::resolve(&v, Path::new("n.md"), "link/a.png").is_err());
    assert!(attachments::resolve(&v, Path::new("n.md"), "alias.png").is_err());
    std::os::unix::fs::symlink(outside.path(), d.path().join("attachments"))?;
    assert!(attachments::import(&v, Path::new("n.md"), &png()).is_err());
    assert!(!outside.path().join("image-1.png").exists());
    assert!(v.entries(Path::new("")).is_ok_and(|e| e.is_empty()));
    let c = Catalog::new(v);
    assert!(wait(&c, |_| true).images.is_empty());
    Ok(())
}
#[test]
fn all_formats_file_drop_read_and_first_frame_decode() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    for (ext, bytes) in [
        ("png", &include_bytes!("fixtures/sample.png")[..]),
        ("jpg", &include_bytes!("fixtures/sample.jpg")[..]),
        ("gif", &include_bytes!("fixtures/sample.gif")[..]),
        ("webp", &include_bytes!("fixtures/sample.webp")[..]),
    ] {
        let path = d.path().join(format!("source.{ext}"));
        fs::write(&path, bytes)?;
        let original = fs::read(&path)?;
        let decoded = images::decode(&images::read(&path)?)?;
        assert_eq!(decoded.original, (48, 32));
        let reference = attachments::import(&v, Path::new("n.md"), &images::read(&path)?)?;
        assert!(reference.ends_with(&format!(".{ext})")));
        assert_eq!(fs::read(&path)?, original);
    }
    Ok(())
}
#[cfg(windows)]
#[test]
fn junction_images_are_neither_indexed_nor_resolved_outside() -> anyhow::Result<()> {
    let d = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let v = Vault::open(d.path())?;
    fs::write(outside.path().join("secret.png"), png())?;
    let link = d.path().join("attachments");
    let result = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()?;
    assert!(result.status.success());
    assert!(v.entries(Path::new(""))?.is_empty());
    assert!(attachments::resolve(&v, Path::new("n.md"), "attachments/secret.png").is_err());
    assert!(attachments::import(&v, Path::new("n.md"), &png()).is_err());
    assert!(!outside.path().join("image-1.png").exists());
    let c = Catalog::new(v);
    assert!(wait(&c, |_| true).images.is_empty());
    drop(c);
    fs::remove_dir(link)?;
    Ok(())
}

#[test]
fn native_bitmap_clipboard_converts_to_png_without_resizing() {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    let original = RgbaImage::from_pixel(2100, 2, Rgba([0, 255, 0, 255]));
    let mut bmp = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(original.clone())
        .write_to(&mut bmp, ImageFormat::Bmp)
        .unwrap();
    let png = nxmd::images::clipboard_bytes(bmp.get_ref()).unwrap();
    assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
    assert_eq!(
        image::load_from_memory(&png).unwrap().into_rgba8(),
        original
    );
    assert!(nxmd::images::decode(bmp.get_ref()).is_err()); // BMP is clipboard-only.
    let d = tempfile::tempdir().unwrap();
    let v = nxmd::vault::Vault::open(d.path()).unwrap();
    assert_eq!(
        nxmd::attachments::import(&v, std::path::Path::new("Note.md"), &png).unwrap(),
        "![](attachments/image-1.png)"
    );
    assert_eq!(
        std::fs::read(d.path().join("attachments/image-1.png")).unwrap(),
        png
    );
    assert!(nxmd::images::clipboard_bytes(b"BMbroken").is_err());
}
