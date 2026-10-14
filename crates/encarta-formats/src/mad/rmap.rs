//! `Terrain` section: the RMAP raster pyramid (mad.md §5).

use super::bits::BitReader;
use super::huffman::{HuffTable, SymbolWidth};
use super::index::Index;
use super::toc::cstr;
use anyhow::{Context, Result, bail, ensure};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Tile side in pixels.
pub const TILE: usize = 127;
/// Pixels per tile.
pub const TILE_PIXELS: usize = TILE * TILE;

const RMAP_VERSION: u32 = 0x0002_0001;
const RAST_ENTRY: usize = 0xD0;

fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).with_context(|| format!("read past end at {o}"))
}
fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).with_context(|| format!("read past end at {o}"))
}
fn f32_at(b: &[u8], o: usize) -> Result<f32> {
    Ok(f32::from_bits(u32_at(b, o)?))
}
fn slice<'b>(b: &'b [u8], off: u32, len: u32, what: &str) -> Result<&'b [u8]> {
    b.get(off as usize..off as usize + len as usize).with_context(|| format!("{what} ({off}+{len}) outside its chunk"))
}

/// One CMT entry: the colour of a raster value.
#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct ValueStyle {
    pub rgb: [u8; 3],
    /// Hill-shading step (low 4 bits).
    pub shade: u8,
    /// The pixel class of the colour (mad-styling.md §5.2); in HYPSO 0 = water, 2 = land and ice (mad.md §5.4).
    pub kind: u8,
}

/// A LOOK colour style.
#[derive(Debug, Clone, Serialize)]
pub struct Look {
    pub name: String,
    /// The RCT legend palette (distinct colours).
    pub legend: Vec<[u8; 3]>,
    /// Colour of each raster value.
    #[serde(skip)]
    pub values: Box<[ValueStyle; 256]>,
}

impl Look {
    /// Renders an index image to RGBA.
    pub fn to_rgba(&self, pixels: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(pixels.len() * 4);
        for &v in pixels {
            let s = &self.values[v as usize];
            out.extend([s.rgb[0], s.rgb[1], s.rgb[2], 255]);
        }
        out
    }
}

fn parse_look(b: &[u8]) -> Result<Vec<Look>> {
    ensure!(b.get(0..4) == Some(b"LOOK"), "LOOK magic missing");
    let n = u32_at(b, 4)? as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let e = 8 + i * 48;
        let name = cstr(b.get(e..e + 32).context("LOOK entry truncated")?);
        let rct = slice(b, u32_at(b, e + 32)?, u32_at(b, e + 36)?, "RCT")?;
        let cmt = slice(b, u32_at(b, e + 40)?, u32_at(b, e + 44)?, "CMT")?;
        ensure!(rct.get(0..4) == Some(b"RCT\0") && rct.len() >= 8 + 1024, "style {name}: bad RCT");
        ensure!(cmt.get(0..4) == Some(b"CMT\0") && cmt.len() >= 4 + 2048, "style {name}: bad CMT");
        let ncolors = (u32_at(rct, 4)? as usize).min(256);
        let legend = (0..ncolors).map(|k| [rct[8 + 4 * k + 2], rct[8 + 4 * k + 1], rct[8 + 4 * k]]).collect();
        let mut values = Box::new([ValueStyle::default(); 256]);
        for (v, s) in values.iter_mut().enumerate() {
            let c = &cmt[4 + 8 * v..12 + 8 * v];
            *s = ValueStyle { rgb: [c[2], c[1], c[0]], shade: c[4] & 0x0F, kind: c[5] };
        }
        out.push(Look { name, legend, values });
    }
    Ok(out)
}

/// The Huffman tables of one tile model.
#[derive(Debug, Clone)]
struct TileModel {
    run: Vec<Arc<HuffTable>>,
    value: Vec<Arc<HuffTable>>,
    long_run: Arc<HuffTable>,
}

fn parse_models(b: &[u8]) -> Result<Vec<TileModel>> {
    let magic = u32_at(b, 0)?;
    ensure!(magic | 7 == 0x1234_FB07, "tile model magic {magic:#x}");
    let dir = u32_at(b, 8)? as usize;
    let area = u32_at(b, 12)? as usize;
    let (w, h, n) = (u32_at(b, 16)?, u32_at(b, 20)?, u32_at(b, 24)?);
    ensure!(w as usize == TILE && h as usize == TILE && n as usize == TILE_PIXELS, "tile size {w}×{h}");
    let nmodels = u16_at(b, dir)? as usize;
    ensure!(nmodels < 3, "{nmodels} tile models");
    let mut cache: HashMap<(usize, bool), Arc<HuffTable>> = HashMap::new();
    let mut table = |entry: u16, wide: bool| -> Result<Arc<HuffTable>> {
        let anchor = area + 4 * entry as usize;
        if let Some(t) = cache.get(&(anchor, wide)) {
            return Ok(t.clone());
        }
        let width = if wide { SymbolWidth::U16 } else { SymbolWidth::U8 };
        let t = Arc::new(HuffTable::parse(b, anchor, width)?);
        cache.insert((anchor, wide), t.clone());
        Ok(t)
    };
    let mut models = Vec::new();
    for m in 0..nmodels {
        let mo = dir + u16_at(b, dir + 2 + 2 * m)? as usize;
        let nlists = u16_at(b, mo)? as usize;
        ensure!((3..=16).contains(&nlists), "model {m}: {nlists} lists");
        let mut lists = Vec::new();
        for l in 0..3 {
            let lo = dir + u16_at(b, mo + 2 + 2 * l)? as usize;
            let nctx = u16_at(b, lo)? as usize;
            let mut tables = Vec::with_capacity(nctx);
            for c in 0..nctx {
                tables.push(table(u16_at(b, lo + 2 + 2 * c)?, l == 2).with_context(|| format!("model {m} list {l} context {c}"))?);
            }
            lists.push(tables);
        }
        let long = lists.pop().unwrap();
        let value = lists.pop().unwrap();
        let run = lists.pop().unwrap();
        ensure!(run.len() == 256 && value.len() == 256 && !long.is_empty(), "model {m}: unexpected context counts");
        models.push(TileModel { run, value, long_run: long[0].clone() });
    }
    Ok(models)
}

/// The INDX key of tile `(x, y)` at `level`: the bits of `x` and `y` interleaved, `x` first.
pub fn tile_key(level: u32, x: u32, y: u32) -> u32 {
    let mut m = 0u32;
    for b in (0..=level).rev() {
        m = (m << 2) | ((x >> b) & 1) << 1 | ((y >> b) & 1);
    }
    level << 28 | m
}

/// Inverse of `tile_key`.
pub fn tile_of_key(key: u32) -> (u32, u32, u32) {
    let level = key >> 28;
    let m = key & 0x0FFF_FFFF;
    let (mut x, mut y) = (0, 0);
    for b in 0..=level {
        x |= ((m >> (2 * b + 1)) & 1) << b;
        y |= ((m >> (2 * b)) & 1) << b;
    }
    (level, x, y)
}

/// Tiles per side at `level`.
pub fn tiles_per_side(level: u32) -> u32 {
    2 << level
}

/// Side in pixels of the full image of `level`.
pub fn image_side(level: u32) -> usize {
    TILE * tiles_per_side(level) as usize
}

/// Whether tile `(x, y)` is stored rotated (the top-right and bottom-left quadrants).
pub fn tile_is_rotated(level: u32, x: u32, y: u32) -> bool {
    let half = 1 << level;
    (x < half) != (y < half)
}

/// Image position `(row, col)` of tile-buffer pixel `(a, b)` of tile `(x, y)`.
pub fn tile_pixel_to_image(level: u32, x: u32, y: u32, a: usize, b: usize) -> (usize, usize) {
    let (r0, c0) = (x as usize * TILE, y as usize * TILE);
    if tile_is_rotated(level, x, y) { (r0 + TILE - 1 - b, c0 + a) } else { (r0 + a, c0 + b) }
}

/// Tile `(x, y)` and tile-buffer index of image pixel `(row, col)`.
pub fn image_to_tile_pixel(level: u32, row: usize, col: usize) -> (u32, u32, usize) {
    let (x, y) = ((row / TILE) as u32, (col / TILE) as u32);
    let (ra, cb) = (row % TILE, col % TILE);
    let (a, b) = if tile_is_rotated(level, x, y) { (cb, TILE - 1 - ra) } else { (ra, cb) };
    (x, y, a * TILE + b)
}

/// A tile buffer rearranged into image orientation (rotated quadrants turned upright).
pub fn tile_in_image_orientation(level: u32, x: u32, y: u32, tile: &[u8]) -> Vec<u8> {
    if !tile_is_rotated(level, x, y) {
        return tile.to_vec();
    }
    let mut out = vec![0u8; TILE_PIXELS];
    for a in 0..TILE {
        for b in 0..TILE {
            out[(TILE - 1 - b) * TILE + a] = tile[a * TILE + b];
        }
    }
    out
}

/// How a tile record was stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TileMode {
    Fill,
    Raw,
    /// RLE with model 0, then transposed.
    RleTransposed,
    /// RLE with model 1.
    Rle,
}

#[derive(Debug, Clone)]
pub struct DecodedTile {
    pub mode: TileMode,
    pub pixels: Vec<u8>,
    /// Unread bits after the last pixel (compressed tiles).
    pub padding_bits: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RasterInfo {
    pub name: String,
    pub max_level: u32,
    pub max_tile_len: u32,
    pub display_scale: f32,
    pub level_caps: Vec<(f32, u32)>,
    pub looks: Vec<String>,
    pub tile_count: u64,
}

pub struct Raster<'a> {
    pub name: String,
    pub max_level: u32,
    pub max_tile_len: u32,
    pub display_scale: f32,
    /// `(scale, level)` overrides.
    pub level_caps: Vec<(f32, u32)>,
    pub looks: Vec<Look>,
    pub index: Index,
    tiles: &'a [u8],
    models: Vec<TileModel>,
}

impl<'a> Raster<'a> {
    pub fn info(&self) -> RasterInfo {
        RasterInfo {
            name: self.name.clone(),
            max_level: self.max_level,
            max_tile_len: self.max_tile_len,
            display_scale: self.display_scale,
            level_caps: self.level_caps.clone(),
            looks: self.looks.iter().map(|l| l.name.clone()).collect(),
            tile_count: self.index.key_count(),
        }
    }

    pub fn look(&self, name: &str) -> Option<&Look> {
        self.looks.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn tile_record(&self, key: u32) -> Result<&'a [u8]> {
        let Some((o, l)) = self.index.lookup(key) else { bail!("{}: no tile {key:#010x}", self.name) };
        slice(self.tiles, o, l, "tile record")
    }

    /// Decodes the tile buffer `O` of tile `(x, y)` at `level` (stored orientation).
    pub fn tile(&self, level: u32, x: u32, y: u32) -> Result<Vec<u8>> {
        Ok(self.decode_key(tile_key(level, x, y))?.pixels)
    }

    pub fn decode_key(&self, key: u32) -> Result<DecodedTile> {
        let rec = self.tile_record(key)?;
        decode_tile(rec, &self.models).with_context(|| format!("{} tile {key:#010x}", self.name))
    }

    /// The full octahedral index image of `level` (`image_side(level)` square, row-major).
    pub fn assemble(&self, level: u32) -> Result<Vec<u8>> {
        ensure!(level <= self.max_level, "{}: level {level} > {}", self.name, self.max_level);
        let n = tiles_per_side(level);
        let side = image_side(level);
        let mut image = vec![0u8; side * side];
        // One band of tile rows at a time keeps the parallel writes disjoint.
        image.par_chunks_mut(side * TILE).enumerate().try_for_each(|(x, band)| -> Result<()> {
            let x = x as u32;
            for y in 0..n {
                let tile = self.tile(level, x, y)?;
                for a in 0..TILE {
                    for b in 0..TILE {
                        let (r, c) = tile_pixel_to_image(level, x, y, a, b);
                        band[(r - x as usize * TILE) * side + c] = tile[a * TILE + b];
                    }
                }
            }
            Ok(())
        })?;
        Ok(image)
    }
}

/// Resamples an octahedral index image (`side` square) to an equirectangular one of
/// `width × width/2` pixels (nearest neighbour; row 0 at 90° N, column 0 at 180° W).
pub fn octahedral_to_equirect(image: &[u8], side: usize, width: usize) -> Vec<u8> {
    octahedral_to_window(image, side, width, width / 2, (-180.0, -90.0, 180.0, 90.0))
}

/// Resamples an octahedral index image to a `width × height` latitude/longitude window
/// `(west, south, east, north)` in degrees (nearest neighbour).
pub fn octahedral_to_window(image: &[u8], side: usize, width: usize, height: usize, window: (f64, f64, f64, f64)) -> Vec<u8> {
    let (w, s, e, n) = window;
    let mut out = vec![0u8; width * height];
    out.par_chunks_mut(width).enumerate().for_each(|(i, row)| {
        let lat = n - (i as f64 + 0.5) * (n - s) / height as f64;
        for (j, px) in row.iter_mut().enumerate() {
            let lon = w + (j as f64 + 0.5) * (e - w) / width as f64;
            let (r, c) = super::octahedral::pixel_of(lat, lon, side as u32);
            *px = image[r as usize * side + c as usize];
        }
    });
    out
}

/// Decodes one tile record with the raster's models.
fn decode_tile(rec: &[u8], models: &[TileModel]) -> Result<DecodedTile> {
    if rec.len() == 1 {
        return Ok(DecodedTile { mode: TileMode::Fill, pixels: vec![rec[0]; TILE_PIXELS], padding_bits: 0 });
    }
    ensure!(rec.len() >= 4, "tile record of {} bytes", rec.len());
    let mut bits = BitReader::new(rec);
    let mode = bits.read(8);
    let (model, transpose, mode) = match mode {
        0 => {
            let raw = rec.get(4..4 + TILE_PIXELS).context("raw tile truncated")?;
            return Ok(DecodedTile { mode: TileMode::Raw, pixels: raw.to_vec(), padding_bits: 0 });
        }
        2 => (0, true, TileMode::RleTransposed),
        3 => (1, false, TileMode::Rle),
        m => bail!("unsupported tile mode {m}"),
    };
    let model = models.get(model).with_context(|| format!("tile model {model} missing"))?;
    let mut scan = vec![0u8; TILE_PIXELS];
    rle(&mut bits, model, &mut scan)?;
    let left = bits.remaining();
    ensure!(left >= 0, "tile decode read {} bits past the end", -left);
    let pixels = if transpose {
        let mut o = vec![0u8; TILE_PIXELS];
        for i in 0..TILE {
            for j in 0..TILE {
                o[i * TILE + j] = scan[j * TILE + i];
            }
        }
        o
    } else {
        scan
    };
    Ok(DecodedTile { mode, pixels, padding_bits: left as u32 })
}

/// Run-length decoding with value/run contexts (mad.md §5.6). A run reaching the end of a row
/// continues on the next row with the value that starts the row just completed.
fn rle(bits: &mut BitReader, m: &TileModel, scan: &mut [u8]) -> Result<()> {
    let mut pos = 0usize;
    let mut col = TILE;
    let (mut prev_value, mut prev_run) = (0usize, 0usize);
    while pos < TILE_PIXELS {
        let v = m.value[prev_value].decode(bits)? as u8;
        let r = m.run[prev_run].decode(bits)? as usize;
        if r == 1 {
            scan[pos] = v;
            pos += 1;
            col -= 1;
            if col == 0 {
                col = TILE;
            }
        } else {
            let mut k = if r == 255 { 255 + m.long_run.decode(bits)? as usize } else { r };
            let mut fill = v;
            while k > 0 {
                let c = k.min(col);
                ensure!(pos + c <= TILE_PIXELS, "tile run overflows the tile");
                scan[pos..pos + c].fill(fill);
                pos += c;
                k -= c;
                col -= c;
                if col == 0 {
                    col = TILE;
                    fill = scan[pos - TILE];
                }
            }
        }
        prev_value = v as usize;
        prev_run = r;
    }
    Ok(())
}

/// The parsed `Terrain` section.
pub struct Terrain<'a> {
    pub rasters: Vec<Raster<'a>>,
    /// TESS globe-mesh vertices (unit vectors).
    pub mesh_vertices: Vec<[f32; 3]>,
    pub header_unknown: (u32, f32),
}

impl<'a> Terrain<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Terrain<'a>> {
        ensure!(b.get(0..4) == Some(b"RMAP"), "Terrain section is not RMAP");
        let version = u32_at(b, 4)?;
        ensure!(version == RMAP_VERSION, "RMAP version {version:#x}");
        let tess = slice(b, u32_at(b, 0x14)?, u32_at(b, 0x18)?, "TESS")?;
        let rast_off = u32_at(b, 0x1C)?;
        let rast = slice(b, rast_off, u32_at(b, 0x20)?, "RAST")?;
        // Entry offsets are relative to the RAST start but point past the RAST chunk itself
        // (LOOK blocks, indexes, tile data and models follow it in the section).
        let base = &b[rast_off as usize..];
        let mesh_vertices = parse_tess(tess)?;
        ensure!(rast.get(0..4) == Some(b"RAST"), "RAST magic missing");
        let n = u32_at(rast, 4)? as usize;
        let mut rasters = Vec::with_capacity(n);
        for i in 0..n {
            let e = 8 + i * RAST_ENTRY;
            let entry = rast.get(e..e + RAST_ENTRY).context("RAST entry truncated")?;
            let name = cstr(&entry[0..32]);
            let r = (|| -> Result<Raster<'a>> {
                let ncaps = u32_at(entry, 0x30)? as usize;
                ensure!(ncaps <= 16, "{ncaps} scale caps");
                let level_caps =
                    (0..ncaps).map(|k| Ok((f32_at(entry, 0x50 + 8 * k)?, u32_at(entry, 0x54 + 8 * k)?))).collect::<Result<_>>()?;
                // RCT/CMT offsets are relative to the LOOK start and lie past the LOOK length.
                let look_off = u32_at(entry, 0x28)? as usize;
                slice(base, look_off as u32, u32_at(entry, 0x2C)?, "LOOK")?;
                let looks = parse_look(&base[look_off..])?;
                let index = Index::parse(slice(base, u32_at(entry, 0x38)?, u32_at(entry, 0x3C)?, "INDX")?)?;
                let tiles = slice(base, u32_at(entry, 0x40)?, u32_at(entry, 0x44)?, "tile data")?;
                ensure!(index.data_end() <= tiles.len() as u64, "index points past the tile data");
                let models = parse_models(slice(base, u32_at(entry, 0x48)?, u32_at(entry, 0x4C)?, "model block")?)?;
                Ok(Raster {
                    name: name.clone(),
                    max_level: u32_at(entry, 0x20)?,
                    max_tile_len: u32_at(entry, 0x24)?,
                    display_scale: f32_at(entry, 0x34)?,
                    level_caps,
                    looks,
                    index,
                    tiles,
                    models,
                })
            })()
            .with_context(|| format!("raster {name}"))?;
            rasters.push(r);
        }
        Ok(Terrain { rasters, mesh_vertices, header_unknown: (u32_at(b, 0x0C)?, f32_at(b, 0x10)?) })
    }

    pub fn raster(&self, name: &str) -> Option<&Raster<'a>> {
        self.rasters.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }
}

fn parse_tess(t: &[u8]) -> Result<Vec<[f32; 3]>> {
    ensure!(t.get(0..4) == Some(b"TESS"), "TESS magic missing");
    let verts = slice(t, u32_at(t, 4)?, u32_at(t, 8)?, "TESS vertices")?;
    Ok(verts
        .as_chunks::<16>().0.iter()
        .map(|v| std::array::from_fn(|k| f32::from_le_bytes(v[4 * k..4 * k + 4].try_into().unwrap())))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mad::bits::testutil::BitWriter;

    #[test]
    fn key_interleave() {
        assert_eq!(tile_key(0, 1, 0), 2);
        assert_eq!(tile_key(0, 0, 1), 1);
        assert_eq!(tile_key(1, 2, 1), 0x1000_0000 | 0b1001);
        for level in 0..9 {
            let n = tiles_per_side(level);
            for &(x, y) in &[(0, 0), (1, 2 % n), (n - 1, 0), (3 % n, 1), (n - 1, n - 2)] {
                assert_eq!(tile_of_key(tile_key(level, x, y)), (level, x, y));
            }
        }
    }

    #[test]
    fn placement_round_trip() {
        let level = 1;
        for (x, y) in [(0, 0), (0, 3), (3, 0), (2, 2), (1, 2)] {
            for (a, b) in [(0, 0), (0, 126), (126, 0), (5, 77)] {
                let (r, c) = tile_pixel_to_image(level, x, y, a, b);
                assert_eq!(image_to_tile_pixel(level, r, c), (x, y, a * TILE + b));
            }
        }
        // Rotated tiles turn 90° counter-clockwise: buffer (0, 0) lands bottom-left.
        assert!(tile_is_rotated(level, 0, 2));
        assert_eq!(tile_pixel_to_image(level, 0, 2, 0, 0), (126, 254));
    }

    /// A model where value tables give 8-bit codes = value and run tables 8-bit codes = run, so
    /// streams are easy to write; long runs use a 16-bit code.
    fn flat_models() -> Vec<TileModel> {
        let lens = |len: u32, n: usize| HuffTable::canonical(&[(len, n)], (0..n).map(|i| i as u16).collect());
        let value = Arc::new(lens(8, 256));
        let run = Arc::new(lens(8, 256));
        let long = Arc::new(lens(16, 65536));
        let m = TileModel { run: vec![run; 256], value: vec![value; 256], long_run: long };
        vec![m.clone(), m]
    }

    fn encode(mode: u32, pairs: &[(u8, u8, Option<u16>)]) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.put(mode, 8);
        for &(v, r, long) in pairs {
            w.put(v as u32, 8);
            w.put(r as u32, 8);
            if let Some(l) = long {
                w.put(l as u32, 16);
            }
        }
        let mut b = w.finish();
        // The mode byte is the MSB of the first LE word: byte 3.
        assert_eq!(b[3] as u32, mode);
        b.extend([0; 4]);
        b
    }

    #[test]
    fn rle_rows_continue_with_the_pixel_above() {
        // Row 0: 0..126 one by one; then a run of 127 + 127·125 + 1 of value 200 that starts row 1
        // (all 200), so later rows continue with the value starting the row above (200); finally
        // a single 7.
        let mut pairs: Vec<(u8, u8, Option<u16>)> = (0..127).map(|v| (v as u8, 1, None)).collect();
        let run = 127 * 126 - 1;
        pairs.push((200, 255, Some((run - 255) as u16)));
        pairs.push((7, 1, None));
        let t = decode_tile(&encode(3, &pairs), &flat_models()).unwrap();
        assert_eq!(t.mode, TileMode::Rle);
        assert_eq!(t.pixels[126], 126);
        assert_eq!(t.pixels[127], 200);
        assert_eq!(t.pixels[TILE_PIXELS - 2], 200);
        assert_eq!(t.pixels[TILE_PIXELS - 1], 7);

        // A run that wraps takes the first pixel of the completed row: row 0 = 5 then 126 × 9.
        let mut pairs = vec![(5, 1, None), (9, 126, None)];
        pairs.push((1, 255, Some((TILE_PIXELS - 127 - 255) as u16)));
        let t = decode_tile(&encode(3, &pairs), &flat_models()).unwrap();
        assert_eq!(&t.pixels[0..3], &[5, 9, 9]);
        // The long run of 1 fills row 1 entirely (it starts at column 0) …
        assert!(t.pixels[127..254].iter().all(|&p| p == 1));
        // … and after that row is complete, the continuation value is row 1's first pixel (1).
        assert!(t.pixels[254..].iter().all(|&p| p == 1));
    }

    #[test]
    fn wrap_uses_the_row_start_value() {
        // 3 single pixels, then a run of 124 + 127 of value 4: row 0 = 1 2 3 4 4 …, row 1 takes
        // the first value of row 0 (1); the remainder is a long run of 6 starting at row 2.
        let mut pairs = vec![(1, 1, None), (2, 1, None), (3, 1, None), (4, 251, None)];
        pairs.push((6, 255, Some((TILE_PIXELS - 254 - 255) as u16)));
        let t = decode_tile(&encode(2, &pairs), &flat_models()).unwrap();
        assert_eq!(t.mode, TileMode::RleTransposed);
        // Undo the transpose to look at the scan.
        let scan = |r: usize, c: usize| t.pixels[c * TILE + r];
        assert_eq!((scan(0, 0), scan(0, 3), scan(0, 126)), (1, 4, 4));
        assert!((0..TILE).all(|c| scan(1, c) == 1));
        assert!((0..TILE).all(|c| scan(2, c) == 6));
    }

    #[test]
    fn fill_and_errors() {
        let t = decode_tile(&[42], &flat_models()).unwrap();
        assert_eq!(t.mode, TileMode::Fill);
        assert!(t.pixels.iter().all(|&p| p == 42));
        assert!(decode_tile(&encode(1, &[]), &flat_models()).is_err());
        // Too many pixels.
        let pairs = vec![(1, 255, Some(u16::MAX)), (1, 255, Some(u16::MAX))];
        assert!(decode_tile(&encode(3, &pairs), &flat_models()).is_err());
    }
}
