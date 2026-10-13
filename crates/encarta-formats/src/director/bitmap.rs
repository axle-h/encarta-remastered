//! Bitmap members: the `CASt` specific data, `BITD` pixel data (run-length coded or raw), `CLUT`
//! palettes, and PNG output. Spec: `docs/formats/shockwave.md` §4 ("Bitmaps").

use super::bytes::{be_i16, be_u16};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

/// Where a bitmap's palette lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum PaletteRef {
    /// a palette member number; `same_cast` when the cast-library field is -1
    Member { number: i16, same_cast: bool },
    /// a built-in palette (-1 System Mac, -101 System Win …)
    BuiltIn(i16),
}

/// Bitmap member header (`CASt` specific data, big-endian).
#[derive(Debug, Clone, Serialize)]
pub struct BitmapInfo {
    pub row_bytes: u16,
    /// top, left, bottom, right
    pub rect: [i16; 4],
    pub reg_point: [i16; 2],
    pub depth: u8,
    pub palette: Option<PaletteRef>,
}

impl BitmapInfo {
    pub fn parse(s: &[u8]) -> Result<Self> {
        let raw = be_u16(s, 0).context("bitmap header")?;
        let rect = [
            be_i16(s, 2).context("rect")?,
            be_i16(s, 4).context("rect")?,
            be_i16(s, 6).context("rect")?,
            be_i16(s, 8).context("rect")?,
        ];
        let reg_point = [be_i16(s, 20).unwrap_or(0), be_i16(s, 18).unwrap_or(0)];
        // bit 15 of the row-bytes word marks the extended (colour) header
        let (depth, palette) = if raw & 0x8000 != 0 && s.len() >= 28 {
            let depth = s[23];
            let lib = be_i16(s, 24).unwrap_or(0);
            let id = be_i16(s, 26).unwrap_or(0);
            let p = if id <= 0 {
                PaletteRef::BuiltIn(if id == 0 { -1 } else { id })
            } else {
                PaletteRef::Member {
                    number: id,
                    same_cast: lib == -1,
                }
            };
            (depth, Some(p))
        } else {
            (1, None)
        };
        Ok(BitmapInfo {
            row_bytes: raw & 0x3fff,
            rect,
            reg_point,
            depth,
            palette,
        })
    }

    pub fn width(&self) -> usize {
        (self.rect[3] - self.rect[1]).max(0) as usize
    }
    pub fn height(&self) -> usize {
        (self.rect[2] - self.rect[0]).max(0) as usize
    }
}

/// 256 RGB entries.
pub type Palette = Vec<[u8; 3]>;

/// `CLUT`: 6 bytes per entry, three big-endian u16 channels (the high byte is the 8-bit value).
pub fn parse_clut(b: &[u8]) -> Palette {
    b.as_chunks::<6>()
        .0
        .iter()
        .map(|e| [e[0], e[2], e[4]])
        .collect()
}

/// The classic Macintosh 8-bit system palette: a 6×6×6 cube (FF..00 in steps of 0x33, red major),
/// then ten-step ramps of red, green, blue and grey without the cube's values, then black.
pub fn mac_system_palette() -> Palette {
    let mut p = Vec::with_capacity(256);
    for i in 0..215u32 {
        let lvl = |n: u32| (0xff - n * 0x33) as u8;
        p.push([lvl(i / 36), lvl(i / 6 % 6), lvl(i % 6)]);
    }
    let ramp: Vec<u8> = (1..=15u8)
        .rev()
        .map(|k| k * 0x11)
        .filter(|v| v % 0x33 != 0)
        .collect();
    for ch in 0..4 {
        for &v in &ramp {
            p.push(match ch {
                0 => [v, 0, 0],
                1 => [0, v, 0],
                2 => [0, 0, v],
                _ => [v, v, v],
            });
        }
    }
    p.push([0, 0, 0]);
    p
}

/// Director's byte-run coding: control byte `c` < 0x80 copies `c + 1` literal bytes; otherwise the
/// next byte repeats `257 - c` times.
pub fn unpack_runs(src: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < src.len() && out.len() < expected {
        let c = src[i];
        i += 1;
        if c < 0x80 {
            let n = (c as usize + 1).min(src.len() - i);
            out.extend_from_slice(&src[i..i + n]);
            i += n;
        } else if i < src.len() {
            let n = 257 - c as usize;
            out.extend(std::iter::repeat_n(src[i], n));
            i += 1;
        }
    }
    out
}

/// Decoded RGBA image.
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Decode `BITD` pixel data. `palette` is used for depths up to 8 (1-bit is black on white).
pub fn decode(info: &BitmapInfo, bitd: &[u8], palette: &Palette) -> Result<Image> {
    let (w, h) = (info.width(), info.height());
    ensure!(w > 0 && h > 0, "empty bitmap");
    let depth = info.depth as usize;
    let pitch = match depth {
        // the stored row bytes are the padded row length for indexed depths
        1 | 2 | 4 | 8 => (info.row_bytes as usize).max((w * depth).div_ceil(8)),
        16 => w * 2,
        32 => w * 4,
        d => bail!("unsupported bit depth {d}"),
    };
    let need = pitch * h;
    let data = if bitd.len() >= need {
        bitd[..need].to_vec()
    } else {
        unpack_runs(bitd, need)
    };
    // A short stream is padded with zero; Director leaves trailing rows empty the same way.
    let mut data = data;
    data.resize(need, 0);

    let mut rgba = vec![0u8; w * h * 4];
    let px = |x: usize, y: usize, c: [u8; 4], rgba: &mut Vec<u8>| {
        let o = (y * w + x) * 4;
        rgba[o..o + 4].copy_from_slice(&c);
    };
    for y in 0..h {
        let row = &data[y * pitch..(y + 1) * pitch];
        for x in 0..w {
            let c = match depth {
                1 => {
                    let bit = row[x / 8] >> (7 - x % 8) & 1;
                    if bit == 1 {
                        [0, 0, 0, 255]
                    } else {
                        [255, 255, 255, 255]
                    }
                }
                2 | 4 => {
                    let per = 8 / depth;
                    let shift = (per - 1 - x % per) * depth;
                    let idx = (row[x / per] >> shift) as usize & ((1 << depth) - 1);
                    let [r, g, b] = palette.get(idx).copied().unwrap_or([0, 0, 0]);
                    [r, g, b, 255]
                }
                8 => {
                    let [r, g, b] = palette.get(row[x] as usize).copied().unwrap_or([0, 0, 0]);
                    [r, g, b, 255]
                }
                16 => {
                    // rows are split: all high bytes, then all low bytes; 1-5-5-5 colour
                    let v = u16::from(row[x]) << 8 | u16::from(row[w + x]);
                    let ch =
                        |s: u16| (((v >> s) & 0x1f) as u8) << 3 | (((v >> s) & 0x1f) as u8) >> 2;
                    [ch(10), ch(5), ch(0), 255]
                }
                _ => {
                    // rows are split into alpha, red, green and blue planes
                    [row[w + x], row[2 * w + x], row[3 * w + x], row[x]]
                }
            };
            px(x, y, c, &mut rgba);
        }
    }
    // 32-bit bitmaps without an alpha channel store zero alpha; show them opaque
    if depth == 32 && rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    Ok(Image {
        width: w,
        height: h,
        rgba,
    })
}

impl Image {
    pub fn to_png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header()?;
            wr.write_image_data(&self.rgba)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_palette_shape() {
        let p = mac_system_palette();
        assert_eq!(p.len(), 256);
        assert_eq!(p[0], [255, 255, 255]);
        assert_eq!(p[214], [0, 0, 0x33]);
        assert_eq!(p[215], [0xee, 0, 0]);
        assert_eq!(p[245], [0xee, 0xee, 0xee]);
        assert_eq!(p[255], [0, 0, 0]);
    }

    #[test]
    fn runs() {
        assert_eq!(
            unpack_runs(&[0x02, 1, 2, 3, 0xfe, 9], 6),
            vec![1, 2, 3, 9, 9, 9]
        );
    }

    #[test]
    fn decode_8bit_runs_with_padding() {
        // 3×2 image, row bytes 4 (padded), palette index 1 = red
        let mut spec = vec![0x80, 4, 0, 0, 0, 0, 0, 2, 0, 3];
        spec.extend_from_slice(&[0; 13]);
        spec.extend_from_slice(&[8, 0xff, 0xff, 0, 1]);
        let info = BitmapInfo::parse(&spec).unwrap();
        assert_eq!((info.width(), info.height(), info.depth), (3, 2, 8));
        assert_eq!(
            info.palette,
            Some(PaletteRef::Member {
                number: 1,
                same_cast: true
            })
        );
        let pal = vec![[255, 255, 255], [255, 0, 0]];
        let img = decode(&info, &[0xf9, 1], &pal).unwrap();
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]);
        assert!(img.to_png().unwrap().starts_with(b"\x89PNG"));
    }

    #[test]
    fn decode_32bit_planes() {
        let mut spec = vec![0x80, 8, 0, 0, 0, 0, 0, 1, 0, 2];
        spec.extend_from_slice(&[0; 13]);
        spec.extend_from_slice(&[32, 0, 0, 0, 0]);
        let info = BitmapInfo::parse(&spec).unwrap();
        // A A R R G G B B (raw, 8 bytes = one row)
        let img = decode(&info, &[0, 0, 10, 20, 30, 40, 50, 60], &Vec::new()).unwrap();
        assert_eq!(img.rgba, vec![10, 30, 50, 255, 20, 40, 60, 255]);
    }
}
