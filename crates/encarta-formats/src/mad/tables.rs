//! Plain (uncompressed) tables (mad.md §8).

use super::g1g2::G1G2;
use super::toc::cstr;
use anyhow::{Result, bail, ensure};
use serde::Serialize;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn fixed_records<'b>(b: &'b [u8], size: usize, name: &str) -> Result<std::slice::ChunksExact<'b, u8>> {
    ensure!(b.len().is_multiple_of(size), "{name}: length {} is not a multiple of {size}", b.len());
    Ok(b.chunks_exact(size))
}

fn cp1252(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    encoding_rs::WINDOWS_1252.decode(&b[..end]).0.into_owned()
}

/// `LangId`: the LCIDs present.
pub fn lang_ids(b: &[u8]) -> Result<Vec<u16>> {
    Ok(fixed_records(b, 2, "LangId")?.map(|r| u16_at(r, 0)).collect())
}

// ---------------------------------------------------------------------------------------------
// PDCAttribute

/// Geometry kind of a feature class.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeometryKind {
    Point,
    RadiusPoint,
    Line,
    Area,
    Other(u8),
}

impl GeometryKind {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => GeometryKind::Point,
            1 => GeometryKind::RadiusPoint,
            2 => GeometryKind::Line,
            3 => GeometryKind::Area,
            o => GeometryKind::Other(o),
        }
    }
    /// Kind 0 parts have exactly one point and no stored point count.
    pub fn has_point_count(self) -> bool {
        self != GeometryKind::Point
    }
}

/// One feature class (62-byte record). GeoGem refers to classes by row position.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureClass {
    pub id: u16,
    pub kind: GeometryKind,
    pub pen_group: u16,
    pub flag: u8,
    pub scale: u32,
    pub reserved: u8,
    pub group: u8,
    pub priority: u8,
    pub name: String,
}

pub fn pdc_attribute(b: &[u8]) -> Result<Vec<FeatureClass>> {
    fixed_records(b, 62, "PDCAttribute")?
        .map(|r| {
            let len = r[2] as usize;
            ensure!(len <= 48, "PDCAttribute name length {len}");
            Ok(FeatureClass {
                id: u16_at(r, 0),
                kind: GeometryKind::from_byte(r[3]),
                pen_group: u16_at(r, 4),
                flag: r[6],
                scale: u32_at(r, 7),
                reserved: r[11],
                group: r[12],
                priority: r[13],
                name: cp1252(&r[14..14 + len]),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Key tables

#[derive(Debug, Clone, Serialize)]
pub struct KeyEntry {
    /// UTF-16 code units (Find Key: collation weights of the page's first key).
    pub text: Option<Vec<u16>>,
    pub value: Option<u32>,
}

/// `Find Key<lcid>` / `EntityConvKey`: `u16 n; u8 has_text; u8 has_value; n × {…}`.
pub fn key_table(b: &[u8]) -> Result<Vec<KeyEntry>> {
    ensure!(b.len() >= 4, "key table too short");
    let n = u16_at(b, 0) as usize;
    let (has_text, has_value) = (b[2] != 0, b[3] != 0);
    let mut pos = 4;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let text = if has_text {
            ensure!(pos < b.len(), "key table entry {i} truncated");
            let len = b[pos] as usize;
            pos += 1;
            ensure!(pos + 2 * len <= b.len(), "key table entry {i} truncated");
            let t = (0..len).map(|k| u16_at(b, pos + 2 * k)).collect();
            pos += 2 * len;
            Some(t)
        } else {
            None
        };
        let value = if has_value {
            ensure!(pos + 4 <= b.len(), "key table entry {i} truncated");
            pos += 4;
            Some(u32_at(b, pos - 4))
        } else {
            None
        };
        out.push(KeyEntry { text, value });
    }
    ensure!(pos == b.len(), "key table: {} trailing bytes", b.len() - pos);
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Find Data

/// Feature types (part 1) and administrative contexts (part 2) of the gazetteer.
#[derive(Debug, Clone, Serialize)]
pub struct FindData {
    /// Indexed by Find Grid `type`.
    pub types: Vec<String>,
    /// Indexed by Find Grid `ctx`.
    pub contexts: Vec<String>,
    pub type_records: Vec<[u16; 5]>,
    pub type_names: Vec<(u16, String)>,
    pub context_nodes: Vec<ContextNode>,
    /// `(node, name, flag)`, sorted by name; flag 0 marks the preferred name.
    pub context_names: Vec<(u32, String, u8)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextNode {
    pub id: u32,
    pub kids: Vec<u32>,
    pub links: Vec<u16>,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let Some(s) = self.b.get(self.pos..self.pos + n) else { bail!("truncated at {} (+{n})", self.pos) };
        self.pos += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16_at(self.take(2)?, 0))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32_at(self.take(4)?, 0))
    }
}

fn pool_string(pool: &[u8], off: u32, g: &G1G2) -> Result<String> {
    let off = off as usize;
    ensure!(off <= pool.len(), "string offset {off} outside the pool of {}", pool.len());
    Ok(g.decode(&pool[off..]))
}

pub fn find_data(b: &[u8], g: &G1G2) -> Result<FindData> {
    let mut r = Reader { b, pos: 0 };
    // Part 1.
    let (s, nrec) = (r.u32()? as usize, r.u32()? as usize);
    let (na, nb, nc, nd) = (r.u16()? as usize, r.u16()? as usize, r.u16()? as usize, r.u16()? as usize);
    let pool = r.take(s)?;
    let recs = r.take(10 * nrec)?;
    let type_records = recs.as_chunks::<10>().0.iter().map(|c| std::array::from_fn(|k| u16_at(c, 2 * k))).collect();
    let mut type_names = Vec::with_capacity(na);
    for _ in 0..na {
        let rec = r.u16()?;
        type_names.push((rec, pool_string(pool, r.u32()?, g)?));
    }
    let mut types = Vec::with_capacity(nb);
    for _ in 0..nb {
        types.push(pool_string(pool, r.u32()?, g)?);
    }
    r.take(nb * (2 + 1 + 2 + 1 + 1))?; // per-type attribute arrays (meaning open)
    r.take(2 * nc + 2 * nd)?;
    // Part 2.
    let (s2, n, k, m) = (r.u32()? as usize, r.u32()? as usize, r.u32()? as usize, r.u32()? as usize);
    let (x, c) = (r.u16()? as usize, r.u16()? as usize);
    let pool2 = r.take(s2)?;
    let nodes = r.take(16 * n)?;
    let kids_raw = r.take(4 * k)?;
    let links_raw = r.take(2 * m)?;
    let mut context_nodes = Vec::with_capacity(n);
    for e in nodes.as_chunks::<16>().0.iter() {
        let (nk, nl) = (u16_at(e, 4) as usize, u16_at(e, 6) as usize);
        let (ks, ls) = (u32_at(e, 8) as usize, u32_at(e, 12) as usize);
        ensure!(ks + nk <= k && ls + nl <= m, "Find Data: context node lists out of range");
        context_nodes.push(ContextNode {
            id: u32_at(e, 0),
            kids: (ks..ks + nk).map(|i| u32_at(kids_raw, 4 * i)).collect(),
            links: (ls..ls + nl).map(|i| u16_at(links_raw, 2 * i)).collect(),
        });
    }
    let mut context_names = Vec::with_capacity(x);
    for e in r.take(9 * x)?.as_chunks::<9>().0.iter() {
        context_names.push((u32_at(e, 0), pool_string(pool2, u32_at(e, 4), g)?, e[8]));
    }
    let mut contexts = Vec::with_capacity(c);
    for _ in 0..c {
        contexts.push(pool_string(pool2, r.u32()?, g)?);
    }
    ensure!(r.pos == b.len(), "Find Data: {} trailing bytes", b.len() - r.pos);
    Ok(FindData { types, contexts, type_records, type_names, context_nodes, context_names })
}

// ---------------------------------------------------------------------------------------------
// EntityCenter

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct EntityCenter {
    pub entity: u32,
    pub cell_key: u32,
    pub x: u16,
    pub y: u16,
}

const CENTER_PAGE: usize = 4092;

/// `u32 npages; u32 first_id[npages]; npages × 4092-byte pages of 12-byte records` (the last page
/// may be short; a record with entity 0 ends a page).
pub fn entity_centers(b: &[u8]) -> Result<Vec<EntityCenter>> {
    ensure!(b.len() >= 4, "EntityCenter too short");
    let npages = u32_at(b, 0) as usize;
    let start = 4 + 4 * npages;
    ensure!(b.len() >= start, "EntityCenter header truncated");
    let mut out = Vec::new();
    for p in 0..npages {
        let first_id = u32_at(b, 4 + 4 * p);
        let ps = start + p * CENTER_PAGE;
        let pe = (ps + CENTER_PAGE).min(b.len());
        ensure!(ps < pe, "EntityCenter page {p} missing");
        let before = out.len();
        for r in b[ps..pe].as_chunks::<12>().0.iter() {
            let entity = u32_at(r, 0);
            if entity == 0 {
                break;
            }
            out.push(EntityCenter { entity, cell_key: u32_at(r, 4), x: u16_at(r, 8), y: u16_at(r, 10) });
        }
        ensure!(out.get(before).is_none_or(|c| c.entity == first_id), "EntityCenter page {p}: first id mismatch");
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Bitmap side tables

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PaletteEntry {
    pub index: u8,
    pub rgb: [u8; 3],
    pub transparent: bool,
}

/// `BitmapPalette`: `{u8 index; u8 r, g, b; u8 transparent}`.
pub fn bitmap_palette(b: &[u8]) -> Result<Vec<PaletteEntry>> {
    Ok(fixed_records(b, 5, "BitmapPalette")?
        .map(|r| PaletteEntry { index: r[0], rgb: [r[1], r[2], r[3]], transparent: r[4] != 0 })
        .collect())
}

/// `BitmapColor`: `{u16 key; 3 colour bytes}` (meaning of the bytes open).
pub fn bitmap_color(b: &[u8]) -> Result<Vec<(u16, [u8; 3])>> {
    Ok(fixed_records(b, 5, "BitmapColor")?.map(|r| (u16_at(r, 0), [r[2], r[3], r[4]])).collect())
}

/// `BitmapDimDir`: `u8 width[n]` then `u8 height[n]`.
pub fn bitmap_dims(b: &[u8]) -> Result<Vec<(u8, u8)>> {
    ensure!(b.len().is_multiple_of(2), "BitmapDimDir length is odd");
    let n = b.len() / 2;
    Ok((0..n).map(|i| (b[i], b[n + i])).collect())
}

// ---------------------------------------------------------------------------------------------
// Styles

#[derive(Debug, Clone, Serialize)]
pub struct MapStyle {
    pub name: String,
    pub raster: String,
    pub look: String,
    pub id: u16,
    /// Three bytes after the id that look like a background colour.
    pub background: [u8; 3],
    #[serde(skip)]
    pub raw: Vec<u8>,
}

pub fn map_styles(b: &[u8]) -> Result<Vec<MapStyle>> {
    Ok(fixed_records(b, 115, "MapStyle")?
        .map(|r| MapStyle {
            name: cp1252(&r[0..26]),
            raster: cstr(&r[0x20..0x2A]),
            look: cstr(&r[0x40..0x4C]),
            id: u16_at(r, 0x60),
            background: [r[0x62], r[0x63], r[0x64]],
            raw: r.to_vec(),
        })
        .collect())
}

/// One `GeoPenDef` record (38 bytes); field meanings mostly open.
#[derive(Debug, Clone, Serialize)]
pub struct PenDef {
    pub head: u16,
    pub b2: u8,
    pub is_head: bool,
    pub u4: u32,
    pub b8: [u8; 4],
    pub w12: u16,
    pub b14: u8,
    pub color: [u8; 3],
    pub flags: [u8; 5],
    pub b23: u8,
    /// Byte 24 minus 0x80.
    pub signed24: i16,
    pub b25: [u8; 5],
}

pub fn pen_defs(b: &[u8]) -> Result<Vec<PenDef>> {
    Ok(fixed_records(b, 38, "GeoPenDef")?
        .map(|r| PenDef {
            head: u16_at(r, 0),
            b2: r[2],
            is_head: r[3] == 1,
            u4: u32_at(r, 4),
            b8: [r[8], r[9], r[10], r[11]],
            w12: u16_at(r, 12),
            b14: r[14],
            color: [r[15], r[16], r[17]],
            flags: [r[18], r[19], r[20], r[21], r[22]],
            b23: r[23],
            signed24: r[24] as i16 - 0x80,
            b25: [r[25], r[26], r[27], r[28], r[29]],
        })
        .collect())
}

/// One `GeoLabel` record (27 bytes), raw layout from mad.md §8.7.
#[derive(Debug, Clone, Serialize)]
pub struct LabelStyle {
    pub b0: u8,
    pub color1: [u8; 3],
    pub b4: [u8; 7],
    /// Byte 11 as (low nibble, high nibble).
    pub nibbles: (u8, u8),
    pub b12: u8,
    pub color2: [u8; 3],
    pub b16: u8,
    pub color3: [u8; 3],
    pub color4: [u8; 3],
    pub tail: [u8; 4],
}

pub fn label_styles(b: &[u8]) -> Result<Vec<LabelStyle>> {
    Ok(fixed_records(b, 27, "GeoLabel")?
        .map(|r| LabelStyle {
            b0: r[0],
            color1: [r[1], r[2], r[3]],
            b4: std::array::from_fn(|k| r[4 + k]),
            nibbles: (r[11] & 15, r[11] >> 4),
            b12: r[12],
            color2: [r[13], r[14], r[15]],
            b16: r[16],
            color3: [r[17], r[18], r[19]],
            color4: [r[20], r[21], r[22]],
            tail: [r[23], r[24], r[25], r[26]],
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelFont {
    pub id: u16,
    pub name: String,
}

/// `Label Font`: `{u16 id, u8 len, UTF-16 name[len]}` until the end.
pub fn label_fonts(b: &[u8]) -> Result<Vec<LabelFont>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < b.len() {
        ensure!(pos + 3 <= b.len(), "Label Font truncated at {pos}");
        let id = u16_at(b, pos);
        let len = b[pos + 2] as usize;
        pos += 3;
        ensure!(pos + 2 * len <= b.len(), "Label Font name truncated at {pos}");
        let units: Vec<u16> = (0..len).map(|k| u16_at(b, pos + 2 * k)).collect();
        pos += 2 * len;
        out.push(LabelFont { id, name: String::from_utf16_lossy(&units) });
    }
    Ok(out)
}

/// `ShieldId`: `{u8 n, u16 value}`.
pub fn shield_ids(b: &[u8]) -> Result<Vec<(u8, u16)>> {
    Ok(fixed_records(b, 3, "ShieldId")?.map(|r| (r[0], u16_at(r, 1))).collect())
}

/// `MillerStyles` (flat-map configuration; layout partly open).
#[derive(Debug, Clone, Serialize)]
pub struct MillerStyles {
    pub header: [u16; 4],
    /// Two `(u16, u16)` pairs per level.
    pub level_pairs: Vec<[(u16, u16); 2]>,
    pub entries: Vec<MillerEntry>,
}

/// One flat-map raster: `raster\0 (look\0)* \0 u32 scale, u8 levels`.
#[derive(Debug, Clone, Serialize)]
pub struct MillerEntry {
    pub raster: String,
    pub looks: Vec<String>,
    pub scale: u32,
    pub levels: u8,
}

pub fn miller_styles(b: &[u8]) -> Result<MillerStyles> {
    ensure!(b.len() >= 8, "MillerStyles too short");
    let header = [u16_at(b, 0), u16_at(b, 2), u16_at(b, 4), u16_at(b, 6)];
    let nlevels = header[3] as usize;
    let mut pos = 8;
    ensure!(b.len() >= pos + 8 * nlevels, "MillerStyles level table truncated");
    let level_pairs = (0..nlevels)
        .map(|l| {
            let o = pos + 8 * l;
            [(u16_at(b, o), u16_at(b, o + 2)), (u16_at(b, o + 4), u16_at(b, o + 6))]
        })
        .collect();
    pos += 8 * nlevels;
    let mut entries = Vec::new();
    while pos < b.len() {
        let mut strings = Vec::new();
        loop {
            let Some(e) = b[pos..].iter().position(|&c| c == 0) else { bail!("MillerStyles: unterminated name at {pos}") };
            let s = String::from_utf8_lossy(&b[pos..pos + e]).into_owned();
            pos += e + 1;
            if s.is_empty() {
                break;
            }
            strings.push(s);
        }
        ensure!(!strings.is_empty() && pos + 5 <= b.len(), "MillerStyles: bad entry before {pos}");
        let raster = strings.remove(0);
        entries.push(MillerEntry { raster, looks: strings, scale: u32_at(b, pos), levels: b[pos + 4] });
        pos += 5;
    }
    Ok(MillerStyles { header, level_pairs, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_tables() {
        let mut b = vec![2, 0, 1, 1];
        b.extend([1, 0x41, 0]);
        b.extend(7u32.to_le_bytes());
        b.extend([0]);
        b.extend(9u32.to_le_bytes());
        let k = key_table(&b).unwrap();
        assert_eq!(k[0].text.as_deref(), Some(&[0x41u16][..]));
        assert_eq!(k[1].value, Some(9));
        b.push(0);
        assert!(key_table(&b).is_err());
    }

    #[test]
    fn centers_pages() {
        let mut b = 2u32.to_le_bytes().to_vec();
        b.extend(5u32.to_le_bytes());
        b.extend(9u32.to_le_bytes());
        let mut page = Vec::new();
        for e in [5u32, 6] {
            page.extend(e.to_le_bytes());
            page.extend((e * 10).to_le_bytes());
            page.extend(1u16.to_le_bytes());
            page.extend(2u16.to_le_bytes());
        }
        page.resize(CENTER_PAGE, 0);
        b.extend(&page);
        b.extend(9u32.to_le_bytes());
        b.extend(90u32.to_le_bytes());
        b.extend([3, 0, 4, 0, 0, 0, 0, 0]);
        let c = entity_centers(&b).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[2], EntityCenter { entity: 9, cell_key: 90, x: 3, y: 4 });
    }

    #[test]
    fn miller_entries() {
        let mut b = vec![0, 1, 0, 1, 0, 0, 1, 0];
        b.extend([2, 0, 0, 0x80, 1, 0, 0, 0x80]);
        b.extend(b"RAS\0LOOKA\0\0");
        b.extend(250u32.to_le_bytes());
        b.push(3);
        b.extend(b"SAT\0sat\0\0");
        b.extend(8000u32.to_le_bytes()); // 40 1F: starts with a printable byte
        b.push(2);
        let m = miller_styles(&b).unwrap();
        assert_eq!(m.level_pairs[0], [(2, 0x8000), (1, 0x8000)]);
        assert_eq!(m.entries.len(), 2);
        assert_eq!((m.entries[0].raster.as_str(), m.entries[0].scale, m.entries[0].levels), ("RAS", 250, 3));
        assert_eq!(m.entries[0].looks, ["LOOKA"]);
        assert_eq!((m.entries[1].looks.len(), m.entries[1].scale), (1, 8000));
    }

    #[test]
    fn label_fonts_and_classes() {
        let mut b = 3u16.to_le_bytes().to_vec();
        b.push(2);
        b.extend([0x41, 0, 0x42, 0]);
        assert_eq!(label_fonts(&b).unwrap()[0].name, "AB");
        let mut r = vec![7, 0, 4, 2, 1, 0, 0];
        r.extend(400000u32.to_le_bytes());
        r.extend([0, 5, 9]);
        r.extend(b"Road");
        r.resize(62, 0);
        let c = pdc_attribute(&r).unwrap();
        assert_eq!((c[0].id, c[0].kind, c[0].scale, c[0].name.as_str()), (7, GeometryKind::Line, 400000, "Road"));
    }
}
