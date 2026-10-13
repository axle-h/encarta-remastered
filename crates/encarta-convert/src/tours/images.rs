//! Image conversion for the tours: model textures, 2D/HUD images, the icon strip and cursors.
//!
//! Rules (spec tours-3d.md §5.5, §8.4.1):
//! - the decoder is chosen by content, never by extension;
//! - JPEGs are kept byte for byte;
//! - BMPs and GIFs become PNG (glTF has no BMP/GIF);
//! - a colour key (magenta, RGB555 test) becomes alpha 0. For 3D textures only BMPs can be keyed;
//!   2D elements key any image when their `color_key` flag is set;
//! - keyed texels get the average colour of their opaque neighbours, so bilinear filtering does not
//!   bleed magenta into cut-out edges (the original's hardware path may have shown such fringes,
//!   spec §5.10).

use anyhow::{Context, Result, bail, ensure};
use encarta_formats::tours::textures::{self, ImageFormat, KeyTest};

/// An RGBA image, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn has_alpha(&self) -> bool {
        self.pixels.iter().any(|p| p[3] != 255)
    }

    /// Makes magenta-key pixels transparent; returns how many were keyed.
    pub fn apply_key(&mut self) -> usize {
        let mut n = 0;
        for p in &mut self.pixels {
            if textures::is_key_magenta([p[0], p[1], p[2]], KeyTest::Rgb555) {
                p[3] = 0;
                n += 1;
            }
        }
        if n > 0 {
            self.bleed_into_transparent();
        }
        n
    }

    /// Gives each fully transparent pixel the mean colour of its opaque 8-neighbours (black when
    /// none), keeping alpha 0.
    fn bleed_into_transparent(&mut self) {
        let (w, h) = (self.width as usize, self.height as usize);
        let src = self.pixels.clone();
        for y in 0..h {
            for x in 0..w {
                if src[y * w + x][3] != 0 {
                    continue;
                }
                let (mut sum, mut n) = ([0u32; 3], 0u32);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let q = src[ny as usize * w + nx as usize];
                        if q[3] != 0 {
                            for k in 0..3 {
                                sum[k] += q[k] as u32;
                            }
                            n += 1;
                        }
                    }
                }
                let c = if n > 0 { sum.map(|s| (s / n) as u8) } else { [0, 0, 0] };
                self.pixels[y * w + x] = [c[0], c[1], c[2], 0];
            }
        }
    }

    /// PNG bytes: RGB when fully opaque, RGBA otherwise.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width, self.height);
            let alpha = self.has_alpha();
            enc.set_color(if alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::High);
            let mut w = enc.write_header()?;
            let data: Vec<u8> = if alpha { self.pixels.iter().flatten().copied().collect() } else { self.pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect() };
            w.write_image_data(&data)?;
        }
        Ok(out)
    }
}

/// Decodes a BMP (the engine's own subset) or the first frame of a GIF (composited on its
/// logical screen; GIF transparency is ignored, as the engine draws GIFs opaque).
pub fn decode(data: &[u8]) -> Result<Rgba> {
    match textures::sniff(data) {
        ImageFormat::Bmp => {
            let b = textures::decode_bmp(data)?;
            Ok(Rgba { width: b.width, height: b.height, pixels: b.rgb.iter().map(|&[r, g, b]| [r, g, b, 255]).collect() })
        }
        ImageFormat::Gif => decode_gif(data),
        ImageFormat::Png => {
            let dec = png::Decoder::new(std::io::Cursor::new(data));
            let mut r = dec.read_info()?;
            let mut buf = vec![0; r.output_buffer_size().context("png size")?];
            let info = r.next_frame(&mut buf)?;
            let px: Vec<[u8; 4]> = match info.color_type {
                png::ColorType::Rgba => buf.as_chunks::<4>().0.to_vec(),
                png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2], 255]).collect(),
                other => bail!("unsupported PNG colour type {other:?}"),
            };
            Ok(Rgba { width: info.width, height: info.height, pixels: px })
        }
        ImageFormat::Jpeg => bail!("JPEGs are kept as they are, not decoded"),
        ImageFormat::Unknown => bail!("unrecognised image data"),
    }
}

fn decode_gif(data: &[u8]) -> Result<Rgba> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut d = opts.read_info(data)?;
    let (sw, sh) = (d.width() as usize, d.height() as usize);
    let global = d.global_palette().map(<[u8]>::to_vec);
    let bg = d.bg_color();
    let frame = d.read_next_frame()?.context("GIF without frames")?.clone();
    let palette = frame.palette.clone().or(global).context("GIF without palette")?;
    let colour = |i: usize| -> [u8; 4] {
        let o = 3 * i;
        if o + 2 < palette.len() { [palette[o], palette[o + 1], palette[o + 2], 255] } else { [0, 0, 0, 255] }
    };
    let fill = bg.map_or([0, 0, 0, 255], colour);
    let mut pixels = vec![fill; sw * sh];
    let (fl, ft, fw) = (frame.left as usize, frame.top as usize, frame.width as usize);
    for (i, &ix) in frame.buffer.iter().enumerate() {
        let (x, y) = (fl + i % fw, ft + i / fw);
        if x < sw && y < sh {
            pixels[y * sw + x] = colour(ix as usize);
        }
    }
    Ok(Rgba { width: sw as u32, height: sh as u32, pixels })
}

/// How a converted image is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub extension: &'static str,
    pub width: u32,
    pub height: u32,
    /// Number of texels made transparent by the colour key.
    pub keyed_texels: usize,
}

/// Converts an image for display. `key`: apply the magenta colour key. JPEGs are never keyed and
/// are returned unchanged.
pub fn convert(data: &[u8], key: bool) -> Result<Converted> {
    if textures::sniff(data) == ImageFormat::Jpeg {
        let info = textures::inspect(data)?;
        return Ok(Converted { bytes: data.to_vec(), mime: "image/jpeg", extension: "jpg", width: info.width, height: info.height, keyed_texels: 0 });
    }
    let mut img = decode(data)?;
    let keyed_texels = if key { img.apply_key() } else { 0 };
    Ok(Converted { bytes: img.to_png()?, mime: "image/png", extension: "png", width: img.width, height: img.height, keyed_texels })
}

/// Model-texture rule: only a BMP whose pixels pass the RGB555 magenta test is keyed.
pub fn texture_is_keyed(data: &[u8]) -> bool {
    textures::sniff(data) == ImageFormat::Bmp && textures::inspect(data).is_ok_and(|i| i.colour_keyed)
}

/// The HUD icon strip: the engine copies BMP rows in file order as top-to-bottom (spec §8.4.1),
/// so the image is the file's rows without the usual bottom-up flip.
pub fn icon_strip(data: &[u8]) -> Result<Rgba> {
    let b = textures::decode_bmp(data)?;
    Ok(Rgba { width: b.width, height: b.height, pixels: b.rows_in_file_order().into_iter().map(|[r, g, b]| [r, g, b, 255]).collect() })
}

/// One decoded cursor image.
#[derive(Debug, Clone)]
pub struct Cursor {
    pub image: Rgba,
    pub hotspot: [u16; 2],
    /// Pixels the AND/XOR masks ask to invert on screen (not representable in a PNG; written as
    /// opaque black). Zero for every shipped cursor is expected.
    pub inverted_pixels: usize,
}

/// Decodes the first image of a Windows `.cur` (DIB with XOR colour bitmap and AND mask, or an
/// embedded PNG).
pub fn decode_cursor(data: &[u8]) -> Result<Cursor> {
    let dir = textures::parse_cursor(data)?;
    let entry = &dir[0];
    let img = &data[entry.offset as usize..(entry.offset + entry.bytes) as usize];
    if img.starts_with(b"\x89PNG") {
        return Ok(Cursor { image: decode(img)?, hotspot: entry.hotspot, inverted_pixels: 0 });
    }
    ensure!(img.len() >= 40, "cursor DIB too short");
    let rd32 = |o: usize| u32::from_le_bytes(img[o..o + 4].try_into().unwrap());
    let rd16 = |o: usize| u16::from_le_bytes(img[o..o + 2].try_into().unwrap());
    let header = rd32(0) as usize;
    let w = rd32(4) as usize;
    let h = (rd32(8) as i32).unsigned_abs() as usize / 2; // XOR + AND
    let bpp = rd16(14) as usize;
    let used = rd32(32) as usize;
    ensure!(rd32(16) == 0, "compressed cursor DIB");
    let pal_n = if bpp <= 8 { if used == 0 { 1 << bpp } else { used } } else { 0 };
    let pal_at = header;
    let xor_at = pal_at + 4 * pal_n;
    let xor_stride = (w * bpp).div_ceil(32) * 4;
    let and_at = xor_at + xor_stride * h;
    let and_stride = w.div_ceil(32) * 4;
    ensure!(and_at + and_stride * h <= img.len(), "cursor DIB truncated");
    let pal = |i: usize| -> [u8; 3] {
        let o = pal_at + 4 * i;
        if i < pal_n { [img[o + 2], img[o + 1], img[o]] } else { [0, 0, 0] }
    };
    let mut pixels = Vec::with_capacity(w * h);
    let mut inverted = 0;
    for y in 0..h {
        let row = h - 1 - y; // bottom-up
        for x in 0..w {
            let xo = xor_at + row * xor_stride;
            let rgb = match bpp {
                1 => pal(((img[xo + x / 8] >> (7 - x % 8)) & 1) as usize),
                4 => pal(((img[xo + x / 2] >> (4 * (1 - x % 2))) & 0xF) as usize),
                8 => pal(img[xo + x] as usize),
                24 => [img[xo + 3 * x + 2], img[xo + 3 * x + 1], img[xo + 3 * x]],
                32 => [img[xo + 4 * x + 2], img[xo + 4 * x + 1], img[xo + 4 * x]],
                other => bail!("unsupported cursor depth {other}"),
            };
            let and = (img[and_at + row * and_stride + x / 8] >> (7 - x % 8)) & 1;
            let px = match (and, rgb) {
                (0, c) => [c[0], c[1], c[2], 255],
                (_, [0, 0, 0]) => [0, 0, 0, 0],
                _ => {
                    inverted += 1;
                    [0, 0, 0, 255]
                }
            };
            pixels.push(px);
        }
    }
    Ok(Cursor { image: Rgba { width: w as u32, height: h as u32, pixels }, hotspot: entry.hotspot, inverted_pixels: inverted })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A bottom-up 24-bit BMP from top-down RGB rows (synthetic).
    pub fn bmp24(w: usize, h: usize, rgb: &[[u8; 3]]) -> Vec<u8> {
        let stride = (w * 3).div_ceil(4) * 4;
        let mut d = Vec::new();
        d.extend_from_slice(b"BM");
        d.extend_from_slice(&((54 + stride * h) as u32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&54u32.to_le_bytes());
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(w as i32).to_le_bytes());
        d.extend_from_slice(&(h as i32).to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        for y in (0..h).rev() {
            let mut row: Vec<u8> = rgb[y * w..(y + 1) * w].iter().flat_map(|p| [p[2], p[1], p[0]]).collect();
            row.resize(stride, 0);
            d.extend_from_slice(&row);
        }
        d
    }

    /// A 2×2 32×32-style cursor: 1-bit DIB, hotspot (1,0); top-left white opaque, top-right
    /// transparent, bottom row black opaque.
    pub fn cursor_1bit() -> Vec<u8> {
        let (w, h) = (2usize, 2usize);
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&(w as u32).to_le_bytes());
        dib.extend_from_slice(&((2 * h) as u32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[0, 0, 0, 0, 255, 255, 255, 0]); // palette: black, white
        // XOR rows bottom-up: bottom row (black, black) then top row (white, black)
        dib.extend_from_slice(&[0b0000_0000, 0, 0, 0]);
        dib.extend_from_slice(&[0b1000_0000, 0, 0, 0]);
        // AND rows bottom-up: bottom (0,0) top (0,1)
        dib.extend_from_slice(&[0b0000_0000, 0, 0, 0]);
        dib.extend_from_slice(&[0b0100_0000, 0, 0, 0]);
        let mut d = Vec::new();
        d.extend_from_slice(&[0, 0, 2, 0, 1, 0]);
        d.extend_from_slice(&[w as u8, h as u8, 0, 0]);
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        d.extend_from_slice(&22u32.to_le_bytes());
        d.extend_from_slice(&dib);
        d
    }

    #[test]
    fn keyed_bmp_becomes_png_with_alpha_and_bleed() {
        let bmp = bmp24(2, 1, &[[255, 0, 255], [10, 20, 30]]);
        assert!(texture_is_keyed(&bmp));
        let c = convert(&bmp, true).unwrap();
        assert_eq!((c.mime, c.width, c.height, c.keyed_texels), ("image/png", 2, 1, 1));
        let back = decode(&c.bytes).unwrap();
        assert_eq!(back.pixels, vec![[10, 20, 30, 0], [10, 20, 30, 255]]);
        let plain = convert(&bmp, false).unwrap();
        assert_eq!(decode(&plain.bytes).unwrap().pixels[0], [255, 0, 255, 255]);
    }

    #[test]
    fn near_magenta_follows_the_555_test() {
        let bmp = bmp24(2, 1, &[[248, 7, 248], [247, 0, 255]]);
        let mut img = decode(&bmp).unwrap();
        assert_eq!(img.apply_key(), 1);
        assert_eq!(img.pixels[1][3], 255);
    }

    #[test]
    fn jpeg_passes_through() {
        let jpg = [0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8, 0, 4, 0, 8, 1, 1, 0x11, 0, 0xFF, 0xD9];
        let c = convert(&jpg, true).unwrap();
        assert_eq!((c.mime, c.width, c.height, c.bytes.as_slice()), ("image/jpeg", 8, 4, &jpg[..]));
    }

    #[test]
    fn icon_strip_keeps_file_row_order() {
        let bmp = bmp24(1, 2, &[[1, 1, 1], [2, 2, 2]]);
        // Displayed top row is [1]; the file stores it last, and the engine shows file rows top-down.
        assert_eq!(icon_strip(&bmp).unwrap().pixels, vec![[2, 2, 2, 255], [1, 1, 1, 255]]);
    }

    #[test]
    fn cursor_masks() {
        let c = decode_cursor(&cursor_1bit()).unwrap();
        assert_eq!(c.hotspot, [1, 0]);
        assert_eq!(c.image.pixels, vec![[255, 255, 255, 255], [0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 255]]);
        assert_eq!(c.inverted_pixels, 0);
    }

    #[test]
    fn gif_first_frame() {
        let mut out = Vec::new();
        {
            let mut e = gif::Encoder::new(&mut out, 2, 1, &[0, 0, 0, 200, 100, 50]).unwrap();
            let f = gif::Frame { width: 2, height: 1, buffer: std::borrow::Cow::Borrowed(&[1, 0]), ..Default::default() };
            e.write_frame(&f).unwrap();
        }
        let img = decode(&out).unwrap();
        assert_eq!(img.pixels, vec![[200, 100, 50, 255], [0, 0, 0, 255]]);
    }
}
