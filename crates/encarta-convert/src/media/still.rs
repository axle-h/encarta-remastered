//! Still images: header facts (size, GIF frames, transparency), BMP/cursor → PNG with the
//! colour-key rules, and the ffmpeg arguments for WebP/AVIF (docs/media.md §2.1).

use super::rules::ColourKey;
use super::sniff::Format;
use anyhow::{Context, Result, bail};
use image::{ImageEncoder, RgbaImage};
use std::io::Cursor;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// GIF frame count (1 for every other format).
    pub frames: u32,
    /// GIF with a transparent colour index in a graphic control extension.
    pub transparent: bool,
}

pub fn info(bytes: &[u8], fmt: Format) -> Result<ImageInfo> {
    if fmt == Format::Gif {
        return gif_info(bytes);
    }
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions().context("reading image header")?;
    Ok(ImageInfo { width, height, frames: 1, transparent: false })
}

/// Walks the GIF block structure (GIF89a spec): counts image descriptors and notes whether any
/// graphic control extension sets the transparency flag.
pub fn gif_info(b: &[u8]) -> Result<ImageInfo> {
    if b.len() < 13 || !(b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        bail!("not a GIF");
    }
    let u16le = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as u32;
    let mut info = ImageInfo { width: u16le(6), height: u16le(8), frames: 0, transparent: false };
    let mut pos = 13;
    if b[10] & 0x80 != 0 {
        pos += 3 << ((b[10] & 7) + 1);
    }
    let skip_sub_blocks = |mut p: usize| -> Option<usize> {
        loop {
            let n = *b.get(p)? as usize;
            p += 1;
            if n == 0 {
                return Some(p);
            }
            p += n;
        }
    };
    while let Some(&block) = b.get(pos) {
        match block {
            0x2C => {
                info.frames += 1;
                let packed = *b.get(pos + 9).context("truncated image descriptor")?;
                pos += 10;
                if packed & 0x80 != 0 {
                    pos += 3 << ((packed & 7) + 1);
                }
                pos += 1; // LZW minimum code size
                pos = skip_sub_blocks(pos).context("truncated image data")?;
            }
            0x21 => {
                let label = *b.get(pos + 1).context("truncated extension")?;
                if label == 0xF9 && b.get(pos + 2) == Some(&4) && b.get(pos + 3).is_some_and(|f| f & 1 != 0) {
                    info.transparent = true;
                }
                pos = skip_sub_blocks(pos + 2).context("truncated extension")?;
            }
            0x3B => break,
            // Trailing garbage after the last frame: stop, as decoders do.
            _ => break,
        }
    }
    if info.frames == 0 {
        bail!("GIF has no image");
    }
    Ok(info)
}

/// Applies a colour key in place; returns the number of pixels made transparent.
pub fn apply_key(img: &mut RgbaImage, key: ColourKey) -> usize {
    let mut n = 0;
    for p in img.pixels_mut() {
        let [r, g, b, _] = p.0;
        let keyed = match key {
            // The flags use (0, 254, 0); match it the same way, as pure green after RGB555.
            ColourKey::Green => r < 8 && g >= 248 && b < 8,
            // RGB555 quantisation keeps the top 5 bits: R and B quantise to 31, G to 0.
            ColourKey::Magenta => r >= 248 && b >= 248 && g < 8,
        };
        if keyed {
            p.0 = [0, 0, 0, 0];
            n += 1;
        }
    }
    n
}

pub struct Png {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub keyed_pixels: usize,
    pub alpha: bool,
}

fn encode_png(img: &RgbaImage, alpha: bool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    );
    if alpha {
        enc.write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)?;
    } else {
        let rgb: Vec<u8> = img.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2]]).collect();
        enc.write_image(&rgb, img.width(), img.height(), image::ExtendedColorType::Rgb8)?;
    }
    Ok(out)
}

/// BMP (any variant the `image` crate reads, including RLE) → PNG. With a key, keyed pixels get
/// alpha 0 and the PNG is RGBA; without one (or when no pixel matches) it is opaque RGB unless
/// the BMP itself carries alpha.
pub fn bmp_to_png(bytes: &[u8], key: Option<ColourKey>) -> Result<Png> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Bmp).context("decoding BMP")?;
    let source_alpha = decoded.color().has_alpha();
    let mut img = decoded.to_rgba8();
    let keyed_pixels = key.map_or(0, |k| apply_key(&mut img, k));
    let alpha = keyed_pixels > 0 || (source_alpha && img.pixels().any(|p| p.0[3] != 255));
    Ok(Png { bytes: encode_png(&img, alpha)?, width: img.width(), height: img.height(), keyed_pixels, alpha })
}

/// Windows cursor → PNG of its largest image, plus the hotspot from the directory entry.
pub fn cursor_to_png(bytes: &[u8]) -> Result<(Png, (u16, u16))> {
    let hotspot = if bytes.len() >= 14 {
        (u16::from_le_bytes([bytes[10], bytes[11]]), u16::from_le_bytes([bytes[12], bytes[13]]))
    } else {
        (0, 0)
    };
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)
        .context("decoding cursor")?
        .to_rgba8();
    let alpha = img.pixels().any(|p| p.0[3] != 255);
    Ok((
        Png { bytes: encode_png(&img, alpha)?, width: img.width(), height: img.height(), keyed_pixels: 0, alpha },
        hotspot,
    ))
}

/// WebP has a 16383-pixel limit per side.
pub const WEBP_MAX: u32 = 16383;

/// ffmpeg output options for a WebP derivative.
pub fn webp_args(encoder: &str, lossless: bool, animated: bool) -> Vec<String> {
    let mut a: Vec<String> = vec!["-c:v".into(), encoder.into()];
    if lossless {
        a.extend(["-lossless", "1", "-compression_level", "6"].map(String::from));
    } else {
        a.extend(["-quality", "80", "-compression_level", "4"].map(String::from));
    }
    if animated {
        a.extend(["-loop", "0"].map(String::from));
    } else {
        a.extend(["-frames:v", "1"].map(String::from));
    }
    a.extend(["-f", "webp"].map(String::from));
    a
}

/// ffmpeg output options for an AVIF derivative (JPEG sources only).
pub fn avif_args(encoder: &str) -> Vec<String> {
    let mut a: Vec<String> = vec!["-c:v".into(), encoder.into(), "-frames:v".into(), "1".into()];
    if encoder == "libaom-av1" {
        a.extend(["-still-picture", "1", "-crf", "30", "-cpu-used", "6", "-threads", "1"].map(String::from));
    } else {
        a.extend(["-crf", "35", "-preset", "8"].map(String::from));
    }
    a.extend(["-pix_fmt", "yuv420p", "-f", "avif"].map(String::from));
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny BMP (24-bit, bottom-up) built by hand.
    fn bmp24(w: u32, h: u32, px: &[[u8; 3]]) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut b = b"BM".to_vec();
        b.extend(size.to_le_bytes());
        b.extend([0u8; 4]);
        b.extend(54u32.to_le_bytes());
        b.extend(40u32.to_le_bytes());
        b.extend((w as i32).to_le_bytes());
        b.extend((h as i32).to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(24u16.to_le_bytes());
        b.extend([0u8; 24]);
        for y in (0..h).rev() {
            let start = b.len();
            for x in 0..w {
                let [r, g, bl] = px[(y * w + x) as usize];
                b.extend([bl, g, r]);
            }
            b.resize(start + row as usize, 0);
        }
        b
    }

    #[test]
    fn magenta_key_uses_555_quantisation() {
        // (255,0,255) and (250,7,249) quantise to magenta in RGB555; (255,8,255) does not.
        let px = [[255, 0, 255], [250, 7, 249], [255, 8, 255], [10, 20, 30]];
        let png = bmp_to_png(&bmp24(2, 2, &px), Some(ColourKey::Magenta)).unwrap();
        assert_eq!((png.width, png.height, png.keyed_pixels, png.alpha), (2, 2, 2, true));
        let img = image::load_from_memory(&png.bytes).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
        assert_eq!(img.get_pixel(1, 0).0[3], 0);
        assert_eq!(img.get_pixel(0, 1).0, [255, 8, 255, 255]);
    }

    #[test]
    fn green_key_and_unkeyed_png_is_opaque() {
        let px = [[0, 255, 0], [8, 255, 0], [0, 254, 0], [0, 247, 0]];
        let png = bmp_to_png(&bmp24(2, 2, &px), Some(ColourKey::Green)).unwrap();
        assert_eq!(png.keyed_pixels, 2);
        let plain = bmp_to_png(&bmp24(2, 2, &px), None).unwrap();
        assert!(!plain.alpha);
        assert_eq!(image::load_from_memory(&plain.bytes).unwrap().color(), image::ColorType::Rgb8);
    }

    #[test]
    fn gif_frames_and_transparency() {
        // Header, 2×1 screen with a 2-colour global table, a GCE with transparency, two frames.
        let mut g = b"GIF89a".to_vec();
        g.extend([2, 0, 1, 0, 0x80, 0, 0]);
        g.extend([0, 0, 0, 255, 255, 255]);
        let frame = |g: &mut Vec<u8>| {
            g.extend([0x2C, 0, 0, 0, 0, 2, 0, 1, 0, 0]);
            g.extend([2, 2, 0x44, 0x01, 0]);
        };
        g.extend([0x21, 0xF9, 4, 1, 0, 0, 0, 0]);
        frame(&mut g);
        frame(&mut g);
        g.push(0x3B);
        let i = gif_info(&g).unwrap();
        assert_eq!((i.width, i.height, i.frames, i.transparent), (2, 1, 2, true));
    }
}
