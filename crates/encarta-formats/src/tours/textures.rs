//! Texture lookup, image sniffing, BMP decoding and the magenta colour key (spec §5.5), plus
//! Windows cursor headers (cursors are kept as they are).

use anyhow::{Result, bail, ensure};
use serde::Serialize;

use super::Reader;

/// Image container, decided by content (never by extension: per `Kevin.jpg` is a BMP and rom
/// `ioban3.bmp` is a GIF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ImageFormat {
    Bmp,
    Jpeg,
    Gif,
    Png,
    Unknown,
}

pub fn sniff(data: &[u8]) -> ImageFormat {
    if data.starts_with(b"BM") {
        ImageFormat::Bmp
    } else if data.starts_with(&[0xFF, 0xD8]) {
        ImageFormat::Jpeg
    } else if data.starts_with(b"GIF8") {
        ImageFormat::Gif
    } else if data.starts_with(b"\x89PNG") {
        ImageFormat::Png
    } else {
        ImageFormat::Unknown
    }
}

/// The usual extension for a sniffed format.
pub fn extension(f: ImageFormat) -> Option<&'static str> {
    match f {
        ImageFormat::Bmp => Some("bmp"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Png => Some("png"),
        ImageFormat::Unknown => None,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// BMP only: bits per pixel.
    pub bits_per_pixel: Option<u16>,
    /// BMP only: some pixel passes the RGB555 magenta test, so the engine colour-keys the texture.
    /// JPEG and GIF textures are never keyed.
    pub colour_keyed: bool,
}

impl ImageInfo {
    pub fn power_of_two(&self) -> bool {
        let p2 = |v: u32| (2..=4096).contains(&v) && v.is_power_of_two();
        p2(self.width) && p2(self.height)
    }
}

/// Sniffs and measures an image; BMPs are decoded to evaluate the colour key.
pub fn inspect(data: &[u8]) -> Result<ImageInfo> {
    let format = sniff(data);
    let (width, height, bpp, keyed) = match format {
        ImageFormat::Bmp => {
            let b = decode_bmp(data)?;
            (b.width, b.height, Some(b.bits_per_pixel), b.has_colour_key(KeyTest::Rgb555))
        }
        ImageFormat::Gif => {
            ensure!(data.len() >= 10, "truncated GIF");
            (u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32, None, false)
        }
        ImageFormat::Jpeg => {
            let (w, h) = jpeg_size(data)?;
            (w, h, None, false)
        }
        ImageFormat::Png => {
            ensure!(data.len() >= 24, "truncated PNG");
            let be = |o: usize| u32::from_be_bytes(data[o..o + 4].try_into().unwrap());
            (be(16), be(20), None, false)
        }
        ImageFormat::Unknown => bail!("unrecognised image data"),
    };
    Ok(ImageInfo { format, width, height, bits_per_pixel: bpp, colour_keyed: keyed })
}

/// Frame size from the first SOF marker.
fn jpeg_size(data: &[u8]) -> Result<(u32, u32)> {
    let mut i = 2;
    while i + 4 <= data.len() {
        ensure!(data[i] == 0xFF, "bad JPEG marker at {i}");
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let sof = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if sof {
            ensure!(i + 9 <= data.len(), "truncated SOF");
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Ok((w, h));
        }
        i += 2 + len;
    }
    bail!("no SOF marker in JPEG")
}

/// Which 16-bit display format the colour key is evaluated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTest {
    /// R≥248, B≥248, G<8.
    Rgb555,
    /// R≥248, B≥248, G<4.
    Rgb565,
}

/// True when the pixel quantises to pure magenta in the given 16-bit format.
pub fn is_key_magenta(rgb: [u8; 3], test: KeyTest) -> bool {
    let g_limit = match test {
        KeyTest::Rgb555 => 8,
        KeyTest::Rgb565 => 4,
    };
    rgb[0] >= 248 && rgb[2] >= 248 && rgb[1] < g_limit
}

/// An uncompressed BMP decoded to RGB rows, top row first (display orientation).
#[derive(Debug, Clone)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub bits_per_pixel: u16,
    /// True when the file stores rows top-down (negative height).
    pub top_down_in_file: bool,
    pub rgb: Vec<[u8; 3]>,
}

impl Bitmap {
    pub fn has_colour_key(&self, test: KeyTest) -> bool {
        self.rgb.iter().any(|&p| is_key_magenta(p, test))
    }

    /// RGBA with alpha 0 on key pixels (the conversion rule for keyed textures: PNG + MASK).
    pub fn to_rgba_keyed(&self, test: KeyTest) -> Vec<[u8; 4]> {
        self.rgb.iter().map(|&[r, g, b]| [r, g, b, if is_key_magenta([r, g, b], test) { 0 } else { 255 }]).collect()
    }

    /// Rows in file order (bottom-up files reversed back). The HUD icon strip is copied this way by
    /// the original, which is why it appears upright (spec §8.4.1).
    pub fn rows_in_file_order(&self) -> Vec<[u8; 3]> {
        if self.top_down_in_file {
            return self.rgb.clone();
        }
        let w = self.width as usize;
        self.rgb.chunks_exact(w.max(1)).rev().flatten().copied().collect()
    }
}

/// Decodes the BMP subset the engine accepts: uncompressed, 8-bit paletted or 24-bit
/// (4- and 32-bit are accepted too, for completeness).
pub fn decode_bmp(data: &[u8]) -> Result<Bitmap> {
    let mut r = Reader::new(data);
    ensure!(r.bytes(2)? == b"BM", "not a BMP");
    let _file_size = r.u32()?;
    let _reserved = r.u32()?;
    let pixel_offset = r.u32()? as usize;
    let header_size = r.u32()? as usize;
    ensure!(header_size >= 40, "unsupported BMP header size {header_size}");
    let width = r.i32()?;
    let height = r.i32()?;
    let _planes = r.u16()?;
    let bpp = r.u16()?;
    let compression = r.u32()?;
    let _image_size = r.u32()?;
    let _xppm = r.i32()?;
    let _yppm = r.i32()?;
    let colours_used = r.u32()? as usize;
    ensure!(compression == 0, "compressed BMP (method {compression}) not supported");
    ensure!(width > 0 && height != 0, "bad BMP size {width}x{height}");
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let palette = if bpp <= 8 {
        let n = if colours_used == 0 { 1 << bpp } else { colours_used };
        let at = 14 + header_size;
        ensure!(at + 4 * n <= data.len(), "truncated palette");
        data[at..at + 4 * n].as_chunks::<4>().0.iter().map(|c| [c[2], c[1], c[0]]).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let stride = (w * bpp as usize).div_ceil(32) * 4;
    ensure!(pixel_offset + stride * h <= data.len(), "truncated BMP pixel data");
    let mut rgb = Vec::with_capacity(w * h);
    for row in 0..h {
        let file_row = if height < 0 { row } else { h - 1 - row };
        let line = &data[pixel_offset + file_row * stride..pixel_offset + (file_row + 1) * stride];
        for x in 0..w {
            let px = match bpp {
                24 => [line[3 * x + 2], line[3 * x + 1], line[3 * x]],
                32 => [line[4 * x + 2], line[4 * x + 1], line[4 * x]],
                8 | 4 => {
                    let idx = if bpp == 8 { line[x] as usize } else { ((line[x / 2] >> (4 * (1 - x % 2))) & 0xF) as usize };
                    *palette.get(idx).unwrap_or(&[0, 0, 0])
                }
                other => bail!("unsupported BMP depth {other}"),
            };
            rgb.push(px);
        }
    }
    Ok(Bitmap { width: w as u32, height: h as u32, bits_per_pixel: bpp, top_down_in_file: height < 0, rgb })
}

/// How a texture name was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Resolution {
    Exact,
    /// The last three characters were replaced by `jpg`.
    JpgFallback,
    /// The last three characters were replaced by `gif`.
    GifFallback,
    Missing,
}

/// Resolves a texture (or any file) name the way the engine does in container mode (spec §5.5):
/// `.ppm` → `.bmp`, directory part dropped, then the name, then the name with its last three
/// characters replaced by `jpg`, then by `gif`. `find` does the case-insensitive lookup and
/// returns the stored file name.
pub fn resolve<'a>(name: &str, find: impl Fn(&str) -> Option<&'a str>) -> (Resolution, Option<&'a str>) {
    let mut name = basename(name).to_string();
    if name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(".ppm") {
        name.truncate(name.len() - 3);
        name.push_str("bmp");
    }
    if let Some(f) = find(&name) {
        return (Resolution::Exact, Some(f));
    }
    // "Last three characters", not the extension; work on chars to stay on a UTF-8 boundary.
    let chars: Vec<char> = name.chars().collect();
    if chars.len() >= 3 {
        let stem: String = chars[..chars.len() - 3].iter().collect();
        for (ext, how) in [("jpg", Resolution::JpgFallback), ("gif", Resolution::GifFallback)] {
            if let Some(f) = find(&format!("{stem}{ext}")) {
                return (how, Some(f));
            }
        }
    }
    (Resolution::Missing, None)
}

/// The part after the last `\` or `/` (container mode reads files by basename).
pub fn basename(name: &str) -> &str {
    name.rsplit(['\\', '/']).next().unwrap_or(name)
}

/// One image of a Windows `.cur` file.
#[derive(Debug, Clone, Serialize)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    pub hotspot: [u16; 2],
    pub bytes: u32,
    pub offset: u32,
}

/// Reads the directory of a `.cur` file (the file itself is kept as it is).
pub fn parse_cursor(data: &[u8]) -> Result<Vec<CursorImage>> {
    let mut r = Reader::new(data);
    ensure!(r.u16()? == 0 && r.u16()? == 2, "not a cursor file");
    let n = r.u16()? as usize;
    ensure!(n > 0, "cursor with no images");
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let w = r.u8()?;
        let h = r.u8()?;
        let _colours = r.u8()?;
        let _reserved = r.u8()?;
        let hx = r.u16()?;
        let hy = r.u16()?;
        let bytes = r.u32()?;
        let offset = r.u32()?;
        ensure!(offset as usize + bytes as usize <= data.len(), "cursor image past the end of the file");
        let dim = |v: u8| if v == 0 { 256 } else { v as u32 };
        out.push(CursorImage { width: dim(w), height: dim(h), hotspot: [hx, hy], bytes, offset });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A bottom-up 24-bit BMP from top-down RGB rows.
    pub fn bmp24(w: usize, h: usize, rgb: &[[u8; 3]]) -> Vec<u8> {
        let stride = (w * 3).div_ceil(4) * 4;
        let mut v = b"BM".to_vec();
        let size = 54 + stride * h;
        v.extend_from_slice(&(size as u32).to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&54u32.to_le_bytes());
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&(w as i32).to_le_bytes());
        v.extend_from_slice(&(h as i32).to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&24u16.to_le_bytes());
        v.extend_from_slice(&[0; 24]);
        for row in (0..h).rev() {
            let mut line = vec![0u8; stride];
            for x in 0..w {
                let [r, g, b] = rgb[row * w + x];
                line[3 * x..3 * x + 3].copy_from_slice(&[b, g, r]);
            }
            v.extend_from_slice(&line);
        }
        v
    }

    #[test]
    fn bmp_decode_and_key() {
        let px = [[255, 0, 255], [10, 20, 30], [250, 6, 250], [0, 0, 0]];
        let b = decode_bmp(&bmp24(2, 2, &px)).unwrap();
        assert_eq!(b.rgb, px);
        assert!(b.has_colour_key(KeyTest::Rgb555));
        let rgba = b.to_rgba_keyed(KeyTest::Rgb565);
        assert_eq!(rgba.iter().map(|p| p[3]).collect::<Vec<_>>(), vec![0, 255, 255, 255], "G=6 passes 555 only");
        assert_eq!(rgba.iter().filter(|p| p[3] == 0).count(), 1);
        assert_eq!(b.to_rgba_keyed(KeyTest::Rgb555).iter().filter(|p| p[3] == 0).count(), 2);
        assert_eq!(b.rows_in_file_order()[0], [250, 6, 250]);
        let info = inspect(&bmp24(2, 2, &px)).unwrap();
        assert_eq!((info.format, info.width, info.colour_keyed, info.power_of_two()), (ImageFormat::Bmp, 2, true, true));
    }

    #[test]
    fn sniff_and_sizes() {
        assert_eq!(sniff(b"GIF89a"), ImageFormat::Gif);
        let gif = [b'G', b'I', b'F', b'8', b'9', b'a', 0x1D, 0x02, 0x7D, 0x01];
        let i = inspect(&gif).unwrap();
        assert_eq!((i.width, i.height, i.colour_keyed), (541, 381, false));
        // SOI, APP0 (len 4), SOF0 with 16x8.
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0, 0xFF, 0xC0, 0x00, 0x0B, 8, 0x00, 0x08, 0x00, 0x10, 3];
        let i = inspect(&jpg).unwrap();
        assert_eq!((i.format, i.width, i.height), (ImageFormat::Jpeg, 16, 8));
    }

    #[test]
    fn resolution_rule() {
        let files = ["Wall.JPG", "fire.gif", "a.bmp"];
        let find = |n: &str| files.iter().copied().find(|f| f.eq_ignore_ascii_case(n));
        assert_eq!(resolve("a.BMP", find), (Resolution::Exact, Some("a.bmp")));
        assert_eq!(resolve("wall.bmp", find), (Resolution::JpgFallback, Some("Wall.JPG")));
        assert_eq!(resolve("fire.bmp", find), (Resolution::GifFallback, Some("fire.gif")));
        assert_eq!(resolve("\\\\share\\dir\\a.ppm", find), (Resolution::Exact, Some("a.bmp")));
        assert_eq!(resolve("nope.jpg", find), (Resolution::Missing, None));
        // Last three characters, not the extension.
        assert_eq!(resolve("fi.b", |n| files.iter().copied().find(|f| f.eq_ignore_ascii_case(n))).0, Resolution::Missing);
    }

    #[test]
    fn cursor_directory() {
        let mut c = vec![0, 0, 2, 0, 1, 0, 32, 32, 0, 0, 5, 0, 6, 0];
        c.extend_from_slice(&4u32.to_le_bytes());
        c.extend_from_slice(&22u32.to_le_bytes());
        c.extend_from_slice(&[0; 4]);
        let imgs = parse_cursor(&c).unwrap();
        assert_eq!((imgs[0].width, imgs[0].hotspot), (32, [5, 6]));
        assert!(parse_cursor(&c[..10]).is_err());
    }
}
