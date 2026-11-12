//! Bounded first-frame decoding shared by imports and native viewers.
use anyhow::{Context, Result, bail};
use std::{
    io::{Cursor, Read},
    path::Path,
};
pub const MAX_ENCODED: usize = 32 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const PREVIEW_EDGE: u32 = 2048;
pub fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    })
}
#[derive(Debug)]
pub struct Decoded {
    pub rgba: image::RgbaImage,
    pub original: (u32, u32),
}
pub fn read(path: &Path) -> Result<Vec<u8>> {
    let f = std::fs::File::open(path).context("Cannot open image")?;
    let mut bytes = Vec::new();
    f.take(MAX_ENCODED as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_ENCODED {
        bail!("Image exceeds 32 MiB encoded limit");
    }
    Ok(bytes)
}
fn decode_full(bytes: &[u8], clipboard: bool) -> Result<image::DynamicImage> {
    if bytes.len() > MAX_ENCODED {
        bail!("Image exceeds 32 MiB encoded limit");
    }
    let format = image::guess_format(bytes).context("Unrecognized image data")?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
    ) && !(clipboard && format == image::ImageFormat::Bmp)
    {
        bail!("Unsupported image format");
    }
    let reader = || image::ImageReader::with_format(Cursor::new(bytes), format);
    let (w, h) = reader().into_dimensions().context("Invalid image header")?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        bail!("Image dimensions exceed the 8192-edge / 16-megapixel limit");
    }
    let mut r = reader();
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    r.limits(limits);
    let image = r
        .decode()
        .context("Image is corrupt or exceeds decoding limits")?;
    Ok(image)
}
/// Windows GPUI exposes CF_DIB as BMP. Convert only clipboard bitmaps to PNG,
/// preserving full dimensions; file imports retain their original encoded bytes.
pub fn clipboard_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    if image::guess_format(bytes)? != image::ImageFormat::Bmp {
        return Ok(bytes.to_vec());
    }
    let image = decode_full(bytes, true)?;
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    let out = out.into_inner();
    if out.len() > MAX_ENCODED {
        bail!("Clipboard PNG exceeds 32 MiB encoded limit");
    }
    Ok(out)
}
pub fn decode(bytes: &[u8]) -> Result<Decoded> {
    let image = decode_full(bytes, false)?;
    let (w, h) = (image.width(), image.height());
    let image = if w > PREVIEW_EDGE || h > PREVIEW_EDGE {
        image.thumbnail(PREVIEW_EDGE, PREVIEW_EDGE)
    } else {
        image
    };
    Ok(Decoded {
        rgba: image.into_rgba8(),
        original: (w, h),
    })
}
/// Fit without upscaling or aspect distortion, independent of window DPI.
pub fn fit(original: (u32, u32), available: (f32, f32)) -> (f32, f32) {
    let (w, h) = (original.0 as f32, original.1 as f32);
    let scale = (available.0.max(0.) / w.max(1.))
        .min(available.1.max(0.) / h.max(1.))
        .min(1.);
    (w * scale, h * scale)
}
