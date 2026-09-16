//! Client-side encrypted thumbnail generation (images + video first frame via Shell).

use crate::error::AppResult;
use image::imageops::FilterType;
use image::DynamicImage;
use std::io::Cursor;
use std::path::Path;

const MAX_EDGE: u32 = 512;
const JPEG_QUALITY: u8 = 70;

/// Returns true when the path looks like an image or video we can thumbnail.
pub fn is_thumbnailable(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff"
            | "mp4" | "mov" | "m4v" | "avi" | "mkv" | "wmv" | "webm"
    )
}

fn is_image_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff"
    )
}

fn encode_jpeg(img: DynamicImage) -> AppResult<Vec<u8>> {
    let rgb = img.into_rgb8();
    let (w, h) = rgb.dimensions();
    let max = w.max(h).max(1);
    let scaled = if max > MAX_EDGE {
        let nw = ((w as u64 * MAX_EDGE as u64) / max as u64).max(1) as u32;
        let nh = ((h as u64 * MAX_EDGE as u64) / max as u64).max(1) as u32;
        image::imageops::resize(&rgb, nw, nh, FilterType::Triangle)
    } else {
        rgb
    };
    let mut out = Cursor::new(Vec::new());
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    encoder
        .encode(
            scaled.as_raw(),
            scaled.width(),
            scaled.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| crate::error::AppError::msg(format!("jpeg encode: {e}")))?;
    Ok(out.into_inner())
}

/// Build a JPEG thumbnail from a local plaintext media file. Soft-fails with `None`.
pub fn generate_jpeg_thumbnail(path: &Path) -> Option<Vec<u8>> {
    if !path.is_file() || !is_thumbnailable(path) {
        return None;
    }

    #[cfg(windows)]
    {
        if let Some(bytes) = shell_item_jpeg(path) {
            return Some(bytes);
        }
    }

    if is_image_ext(path) {
        match image::open(path) {
            Ok(img) => encode_jpeg(img).ok(),
            Err(e) => {
                eprintln!("thumbnail open {}: {e}", path.display());
                None
            }
        }
    } else {
        None
    }
}

#[cfg(windows)]
fn shell_item_jpeg(path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let item: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let size = SIZE {
            cx: MAX_EDGE as i32,
            cy: MAX_EDGE as i32,
        };
        let hbmp = item
            .GetImage(size, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
            .ok()?;

        let mut bmp = BITMAP::default();
        if GetObjectW(
            HGDIOBJ(hbmp.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        ) == 0
        {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return None;
        }
        let width = bmp.bmWidth.max(1) as u32;
        let height = bmp.bmHeight.abs().max(1) as u32;
        let stride = ((width * 3 + 3) & !3) as usize;
        let mut pixels = vec![0u8; stride * height as usize];

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // top-down
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return None;
        }
        let ok = GetDIBits(
            hdc,
            hbmp,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(hdc);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        if ok == 0 {
            return None;
        }

        // BGR packed rows → RGB image
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            let row = &pixels[y * stride..y * stride + (width as usize * 3)];
            for px in row.chunks_exact(3) {
                rgb.push(px[2]);
                rgb.push(px[1]);
                rgb.push(px[0]);
            }
        }
        let img = image::RgbImage::from_raw(width, height, rgb)?;
        encode_jpeg(DynamicImage::ImageRgb8(img)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_media() {
        assert!(is_thumbnailable(Path::new("a.JPG")));
        assert!(is_thumbnailable(Path::new("b.mp4")));
        assert!(!is_thumbnailable(Path::new("c.txt")));
    }
}
