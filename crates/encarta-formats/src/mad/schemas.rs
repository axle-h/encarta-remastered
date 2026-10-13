//! Record schemas of the Grid tables (mad.md §7): which fields are read, in which order.

use super::grid::{Grid, Record};
use super::tables::GeometryKind;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

/// A record type decoded field by field from a grid record.
pub trait GridSchema: Sized {
    fn decode(r: &mut Record) -> Result<Self>;
}

/// Decodes the record for `key` and checks that 0…31 bits are left over.
pub fn decode_record<T: GridSchema>(grid: &Grid, key: u32) -> Result<T> {
    let mut r = grid.record(key)?;
    let v = T::decode(&mut r).with_context(|| format!("{} record {key:#x}", grid.name))?;
    r.finish().with_context(|| format!("{} record {key:#x}", grid.name))?;
    Ok(v)
}

/// Decodes a record from its bytes.
pub fn decode_bytes<T: GridSchema>(grid: &Grid, bytes: &[u8]) -> Result<(T, u32)> {
    let mut r = grid.decoder(bytes);
    let v = T::decode(&mut r)?;
    let left = r.finish()?;
    Ok((v, left))
}

fn utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LegendEntry {
    pub a: u16,
    pub b: u16,
    pub c: u16,
    pub text: String,
}

/// LegendGrid record (key = LCID).
#[derive(Debug, Clone, Serialize)]
pub struct Legend {
    pub entries: Vec<LegendEntry>,
}

impl GridSchema for Legend {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let items = r.words(1, 3 * n)?;
        let total = r.dwords(2, 1)?[0] as usize;
        let text = r.words(6, total)?;
        let strings: Vec<&[u16]> = text.split(|&c| c == 0).collect();
        ensure!(strings.len() > n || (n == 0 && total == 0), "legend: {} strings for {n} entries", strings.len() - 1);
        let entries = (0..n)
            .map(|i| LegendEntry { a: items[3 * i], b: items[3 * i + 1], c: items[3 * i + 2], text: utf16(strings[i]) })
            .collect();
        Ok(Legend { entries })
    }
}

/// Bitmap record (key = symbol id): palette indices, row-major.
#[derive(Debug, Clone, Serialize)]
pub struct Bitmap {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

impl GridSchema for Bitmap {
    fn decode(r: &mut Record) -> Result<Self> {
        let wh = r.words(0, 2)?;
        let pixels = r.bytes(1, wh[0] as usize * wh[1] as usize)?;
        Ok(Bitmap { width: wh[0], height: wh[1], pixels })
    }
}

impl Bitmap {
    /// RGBA using `BitmapPalette`; transparent palette entries get alpha 0.
    pub fn to_rgba(&self, palette: &[super::tables::PaletteEntry]) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &p in &self.pixels {
            match palette.iter().find(|e| e.index == p) {
                Some(e) => out.extend([e.rgb[0], e.rgb[1], e.rgb[2], if e.transparent { 0 } else { 255 }]),
                None => out.extend([255, 0, 255, 255]),
            }
        }
        out
    }
}

/// MillerTerrainGrid record (flat-map tile).
#[derive(Debug, Clone, Serialize)]
pub struct MillerTile {
    pub width: u16,
    pub height: u16,
    pub slot: u16,
    #[serde(skip)]
    pub pixels: Vec<u8>,
}

impl GridSchema for MillerTile {
    fn decode(r: &mut Record) -> Result<Self> {
        let v = r.words(0, 3)?;
        let pixels = r.bytes(1, v[0] as usize * v[1] as usize)?;
        Ok(MillerTile { width: v[0], height: v[1], slot: v[2], pixels })
    }
}

/// Where an entity's GeoObj entry is.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct EntityLocation {
    pub entity: u32,
    pub geoobj_key: u32,
    pub entry: u16,
}

/// EntityConv record (key = page).
#[derive(Debug, Clone, Serialize)]
pub struct EntityConvPage {
    pub entries: Vec<EntityLocation>,
}

impl GridSchema for EntityConvPage {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let ids = r.dwords(1, n)?;
        let p = r.words(5, n)?;
        let q = r.words(6, n)?;
        let d = r.words(7, n)?;
        let entries = (0..n)
            .map(|i| EntityLocation { entity: ids[i], geoobj_key: p[i] as u32 * 4096 + q[i] as u32, entry: d[i] })
            .collect();
        Ok(EntityConvPage { entries })
    }
}

/// FindGECContextGrid record: `(type, context)` pairs.
#[derive(Debug, Clone, Serialize)]
pub struct FindContextPage {
    pub pairs: Vec<(u16, u16)>,
}

impl GridSchema for FindContextPage {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let t = r.words(1, n)?;
        let c = r.words(2, n)?;
        Ok(FindContextPage { pairs: t.into_iter().zip(c).collect() })
    }
}

/// One Find Grid entry (a search key of an entity).
#[derive(Debug, Clone, Serialize)]
pub struct FindEntry {
    pub entity: u32,
    pub kind: u16,
    pub context: u16,
    pub flags: u8,
    pub shared_prefix: u8,
    pub letters: u8,
    /// The stored fragment (G1G2 bytes).
    #[serde(skip)]
    pub fragment: Vec<u8>,
    /// The full key rebuilt from the base and the fragment (G1G2 bytes).
    #[serde(skip)]
    pub key: Vec<u8>,
}

/// Flag: the key continues the current base.
pub const FIND_CONTINUES: u8 = 0x80;

/// The base's first word: up to its first space, hyphen, 0x08 or 0x09 after position 0.
fn first_word(base: &[u8]) -> &[u8] {
    let end = base.iter().skip(1).position(|&c| matches!(c, 0x20 | 0x2D | 0x08 | 0x09)).map_or(base.len(), |p| p + 1);
    &base[..end]
}

/// Find Grid record (key = page).
#[derive(Debug, Clone, Serialize)]
pub struct FindPage {
    pub entries: Vec<FindEntry>,
}

impl GridSchema for FindPage {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let ids = r.dwords(1, n)?;
        let kinds = r.words(5, n)?;
        let ctx = r.words(6, n)?;
        let flags = r.bytes(7, n)?;
        let pre = r.bytes(8, n)?;
        let nlet = r.bytes(9, n)?;
        let hi = r.word(10)? as usize;
        let lo = r.word(11)? as usize;
        let total = hi * 32768 + lo;
        let s = r.bytes(12, total)?;
        let mut strings = s.split(|&c| c == 0);
        let mut base: Vec<u8> = Vec::new();
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let frag = strings.next().with_context(|| format!("Find page: string {i} of {n} missing"))?.to_vec();
            let key = if flags[i] & FIND_CONTINUES == 0 {
                base = frag.clone();
                frag.clone()
            } else if frag.is_empty() {
                base.clone()
            } else {
                let mut k = first_word(&base).to_vec();
                k.extend(&frag);
                k
            };
            entries.push(FindEntry {
                entity: ids[i],
                kind: kinds[i],
                context: ctx[i],
                flags: flags[i],
                shared_prefix: pre[i],
                letters: nlet[i],
                fragment: frag,
                key,
            });
        }
        // The strings are NUL-terminated, so exactly one empty remainder follows the last.
        ensure!(n == 0 || (strings.next() == Some(&[][..]) && strings.next().is_none()), "Find page: extra strings");
        Ok(FindPage { entries })
    }
}

/// One GeoObj entry: a named feature.
#[derive(Debug, Clone, Serialize)]
pub struct GeoObjEntry {
    pub entity: u32,
    pub class: u8,
    /// Display-scale value (`c`), 0 for most small features.
    pub scale: u32,
    /// `(e, f, g)` for entries with a nonzero scale value (meaning open).
    pub label: Option<(u32, u16, u16)>,
    /// Name(s), alternatives separated by U+0004.
    pub name: String,
}

/// GeoObjGrid record (key = spatial cell).
#[derive(Debug, Clone, Serialize)]
pub struct GeoObjRecord {
    pub entries: Vec<GeoObjEntry>,
}

impl GridSchema for GeoObjRecord {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let ids = r.dwords(1, n)?;
        let cls = r.bytes(5, n)?;
        let c = r.dwords(6, n)?;
        let m = c.iter().filter(|&&v| v != 0).count();
        let e = r.dwords(10, m)?;
        let f = r.words(14, m)?;
        let g = r.words(15, m)?;
        let lens = r.bytes(16, n)?;
        let total: usize = lens.iter().map(|&l| l as usize).sum();
        let text = r.words(17, total)?;
        let mut pos = 0;
        let mut k = 0;
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let len = lens[i] as usize;
            let label = if c[i] != 0 {
                k += 1;
                Some((e[k - 1], f[k - 1], g[k - 1]))
            } else {
                None
            };
            entries.push(GeoObjEntry { entity: ids[i], class: cls[i], scale: c[i], label, name: utf16(&text[pos..pos + len]) });
            pos += len;
        }
        Ok(GeoObjRecord { entries })
    }
}

/// A GeoGem object reference: GeoObj record key and entry index.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectRef {
    pub key: u32,
    pub entry: u32,
}

/// Expands the object reference stream into `n` references (mad.md §7.8).
pub fn expand_refs(words: &[u16], n: usize) -> Result<Vec<ObjectRef>> {
    let mut out = Vec::with_capacity(n);
    let (mut key, mut entry) = (0u64, 0u64);
    let mut i = 0;
    let arg = |i: &mut usize| -> Result<u64> {
        let Some(&w) = words.get(*i) else { bail!("object reference stream truncated") };
        *i += 1;
        Ok(w as u64)
    };
    while out.len() < n {
        let w = arg(&mut i)?;
        let produce = match w {
            0x100 => {
                key += 1 + arg(&mut i)?;
                entry = 0;
                0
            }
            0x101 => {
                key += 1 + (arg(&mut i)? << 8) + arg(&mut i)?;
                entry = 0;
                0
            }
            0x102 => {
                key += 1 + (arg(&mut i)? << 16) + (arg(&mut i)? << 8) + arg(&mut i)?;
                entry = 0;
                0
            }
            0x103 => {
                key += 1 + (arg(&mut i)? << 24) + (arg(&mut i)? << 16) + (arg(&mut i)? << 8) + arg(&mut i)?;
                entry = 0;
                0
            }
            0x104 => {
                entry += 1 + arg(&mut i)?;
                0
            }
            0x105 => {
                entry += 1 + (arg(&mut i)? << 8) + arg(&mut i)?;
                0
            }
            0x107 => (arg(&mut i)? << 8) + arg(&mut i)? + 1,
            v => v + 1,
        };
        ensure!(out.len() as u64 + produce <= n as u64, "object reference stream produces more than {n} references");
        ensure!(key <= u32::MAX as u64 && entry + produce <= u32::MAX as u64, "object reference overflow");
        for _ in 0..produce {
            out.push(ObjectRef { key: key as u32, entry: entry as u32 });
            entry += 1;
        }
    }
    ensure!(i == words.len(), "object reference stream: {} unused words", words.len() - i);
    Ok(out)
}

/// Undoes the predictive delta coding of one coordinate stream (mad.md §7.8).
pub struct DeltaDecoder {
    prev: i32,
    cur: i32,
}

impl Default for DeltaDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaDecoder {
    pub fn new() -> Self {
        DeltaDecoder { prev: 0, cur: 0 }
    }

    pub fn step(&mut self, v: u16) -> i32 {
        let v = v as i32;
        let new = if v >= 64 {
            v - 64
        } else {
            let vel = self.cur - self.prev;
            if vel > 4 { self.cur + v - 28 } else { self.cur + vel.max(-4) + v - 32 }
        };
        self.prev = self.cur;
        self.cur = new;
        new
    }
}

/// One part of a GeoGem object: a class and its points.
#[derive(Debug, Clone, Serialize)]
pub struct GeoPart {
    /// PDCAttribute row.
    pub class: u16,
    /// Range into the record's point arrays.
    pub first_point: u32,
    pub point_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeoObject {
    pub object: ObjectRef,
    pub parts: Vec<GeoPart>,
}

/// GeoGemGrid record (key = spatial cell): objects, their parts and points in cell coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct GeoGemRecord {
    pub objects: Vec<GeoObject>,
    /// 0: coordinates were interleaved, 1: all x then all y.
    pub layout: u32,
    #[serde(skip)]
    pub x: Vec<i32>,
    #[serde(skip)]
    pub y: Vec<i32>,
}

impl GeoGemRecord {
    /// `kinds[row]` is the geometry kind of PDCAttribute row `row`.
    pub fn decode(r: &mut Record, kinds: &[GeometryKind]) -> Result<Self> {
        let n = r.word(0)? as usize;
        let m = r.word(1)? as usize;
        let w = r.words(2, m)?;
        let refs = expand_refs(&w, n)?;
        let parts = r.words(3, n)?;
        let total_parts: usize = parts.iter().map(|&p| p as usize).sum();
        let classes = r.words(4, total_parts)?;
        let mut counted = 0;
        for &c in &classes {
            let kind = kinds.get(c as usize).with_context(|| format!("part class {c} not in PDCAttribute"))?;
            if kind.has_point_count() {
                counted += 1;
            }
        }
        let stored = r.words(5, counted)?;
        let mut stored_iter = stored.iter();
        let mut counts = Vec::with_capacity(total_parts);
        for &c in &classes {
            counts.push(if kinds[c as usize].has_point_count() { *stored_iter.next().unwrap() as u32 } else { 1 });
        }
        let total_points: usize = counts.iter().map(|&c| c as usize).sum();
        let (crd, layout) = r.words_ctx(6, 2 * total_points)?;
        let (mut x, mut y) = (Vec::with_capacity(total_points), Vec::with_capacity(total_points));
        let (mut dx, mut dy) = (DeltaDecoder::new(), DeltaDecoder::new());
        match layout {
            0 => {
                for pair in crd.as_chunks::<2>().0.iter() {
                    x.push(dx.step(pair[0]));
                    y.push(dy.step(pair[1]));
                }
            }
            1 => {
                let (xs, ys) = crd.split_at(total_points);
                x.extend(xs.iter().map(|&v| dx.step(v)));
                y.extend(ys.iter().map(|&v| dy.step(v)));
            }
            l => bail!("coordinate layout {l}"),
        }
        let mut objects = Vec::with_capacity(n);
        let (mut part, mut point) = (0usize, 0u32);
        for (i, &object) in refs.iter().enumerate() {
            let mut ps = Vec::with_capacity(parts[i] as usize);
            for _ in 0..parts[i] {
                ps.push(GeoPart { class: classes[part], first_point: point, point_count: counts[part] });
                point += counts[part];
                part += 1;
            }
            objects.push(GeoObject { object, parts: ps });
        }
        Ok(GeoGemRecord { objects, layout, x, y })
    }

    pub fn point_count(&self) -> usize {
        self.x.len()
    }

    pub fn points(&self, p: &GeoPart) -> impl Iterator<Item = (i32, i32)> + '_ {
        let r = p.first_point as usize..(p.first_point + p.point_count) as usize;
        self.x[r.clone()].iter().copied().zip(self.y[r].iter().copied())
    }
}

/// One PDCAltitudeGrid rule: how a feature class is drawn in a scale range.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayRule {
    /// PDCAttribute row.
    pub class: u16,
    pub b2: u8,
    pub b3: u8,
    pub b4: u8,
    /// Fields 5…13 (6…13 are GeoPenDef pen numbers, 0xFFFF = none).
    pub w5_13: [u16; 9],
    pub b14_16: [u8; 3],
    /// Pen number.
    pub w17: u16,
    pub b18_20: [u8; 3],
    /// Pen numbers.
    pub w21_24: [u16; 4],
    /// Scale-denominator range of the rule.
    pub max_scale: u32,
    pub min_scale: u32,
}

/// PDCAltitudeGrid record (key = MapStyle id).
#[derive(Debug, Clone, Serialize)]
pub struct DisplayRules {
    pub rules: Vec<DisplayRule>,
}

impl GridSchema for DisplayRules {
    fn decode(r: &mut Record) -> Result<Self> {
        let n = r.word(0)? as usize;
        let cls = r.words(1, n)?;
        let b2 = r.bytes(2, n)?;
        let b3 = r.bytes(3, n)?;
        let b4 = r.bytes(4, n)?;
        let mut w5 = Vec::new();
        for f in 5..=13 {
            w5.push(r.words(f, n)?);
        }
        let mut b14 = Vec::new();
        for f in 14..=16 {
            b14.push(r.bytes(f, n)?);
        }
        let w17 = r.words(17, n)?;
        let mut b18 = Vec::new();
        for f in 18..=20 {
            b18.push(r.bytes(f, n)?);
        }
        let mut w21 = Vec::new();
        for f in 21..=24 {
            w21.push(r.words(f, n)?);
        }
        let smax = r.dwords(25, n)?;
        let smin = r.dwords(29, n)?;
        let rules = (0..n)
            .map(|i| DisplayRule {
                class: cls[i],
                b2: b2[i],
                b3: b3[i],
                b4: b4[i],
                w5_13: std::array::from_fn(|k| w5[k][i]),
                b14_16: std::array::from_fn(|k| b14[k][i]),
                w17: w17[i],
                b18_20: std::array::from_fn(|k| b18[k][i]),
                w21_24: std::array::from_fn(|k| w21[k][i]),
                max_scale: smax[i],
                min_scale: smin[i],
            })
            .collect();
        Ok(DisplayRules { rules })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mad::bits::testutil::BitWriter;
    use crate::mad::grid::GridModel;
    use crate::mad::grid::testutil::{FieldSpec, flat_table, model};

    #[test]
    fn reference_stream() {
        // 2 refs in record 0, jump to record 3, skip 2 entries, 1 ref, then a counted run of 0x102.
        let w = [1, 0x100, 2, 0x104, 1, 0, 0x101, 0, 4, 0x107, 1, 1];
        let refs = expand_refs(&w, 2 + 1 + 0x102).unwrap();
        assert_eq!(refs[0], ObjectRef { key: 0, entry: 0 });
        assert_eq!(refs[1], ObjectRef { key: 0, entry: 1 });
        assert_eq!(refs[2], ObjectRef { key: 3, entry: 2 });
        assert_eq!(refs[3], ObjectRef { key: 8, entry: 0 });
        assert_eq!(refs.last().unwrap(), &ObjectRef { key: 8, entry: 0x101 });
        assert!(expand_refs(&w, 2).is_err());
        assert!(expand_refs(&w[..3], 3).is_err());
    }

    #[test]
    fn delta_prediction() {
        let mut d = DeltaDecoder::new();
        // Two absolute 100s (velocity 0); +3 → 103; velocity 3: 103+3+0 = 106; 106+3+8 = 117;
        // velocity 11 > 4 predicts +4: 117 + 30 − 28 = 119.
        let got: Vec<i32> = [164, 164, 35, 32, 40, 30].iter().map(|&v| d.step(v)).collect();
        assert_eq!(got, [100, 100, 103, 106, 117, 119]);
        // Negative velocities are capped at −4.
        let mut d = DeltaDecoder::new();
        let got: Vec<i32> = [564, 64 + 400, 32].iter().map(|&v| d.step(v)).collect();
        assert_eq!(got, [500, 400, 396]);
    }

    #[test]
    fn first_word_rule() {
        assert_eq!(first_word(b"Antigua"), b"Antigua");
        assert_eq!(first_word(b"`abil ad Dodah"), b"`abil");
        assert_eq!(first_word(b"-x-y"), b"-x");
        assert_eq!(first_word(b"A\x08B"), b"A");
    }

    /// Writes literals with the "1 bit + 11/16-bit escape" table.
    fn lit(w: &mut BitWriter, v: u32) {
        if v < 2048 {
            w.put(0, 1);
            w.put(v, 11);
        } else {
            w.put(1, 1);
            w.put(v, 16);
        }
    }

    fn escape_model(nfields: usize, ctx_field: Option<usize>) -> Vec<u8> {
        let fields: Vec<FieldSpec> = (0..nfields)
            .map(|f| {
                let mut s = FieldSpec::simple(0);
                if Some(f) == ctx_field {
                    s = FieldSpec { nctx: 2, filter: 0, sentinel: 0, modulus: 2, tables: vec![[0; 4], [0; 4]], phrases: vec![] };
                }
                s
            })
            .collect();
        model(&[flat_table(1, &[0xFFFF, 0xFFFE])], &fields)
    }

    #[test]
    fn geogem_record() {
        let m = GridModel::parse(&escape_model(7, Some(6))).unwrap();
        let kinds = [GeometryKind::Point, GeometryKind::Line];
        let mut w = BitWriter::default();
        lit(&mut w, 2); // n objects
        lit(&mut w, 1); // m words
        lit(&mut w, 1); // produce 2 refs
        lit(&mut w, 1); // object 0: 1 part
        lit(&mut w, 2); // object 1: 2 parts
        for c in [1, 0, 1] {
            lit(&mut w, c);
        }
        lit(&mut w, 2); // line part point counts
        lit(&mut w, 3);
        // 6 points, layout 1 (context 1).
        w.put(1, 1);
        for v in [64 + 10, 64 + 11, 33, 64 + 500, 64 + 20, 31] {
            lit(&mut w, v); // x
        }
        for v in [64, 64 + 1023, 64 + 7, 32, 64, 64] {
            lit(&mut w, v); // y
        }
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        let g = GeoGemRecord::decode(&mut r, &kinds).unwrap();
        r.finish().unwrap();
        assert_eq!(g.layout, 1);
        assert_eq!(g.objects.len(), 2);
        assert_eq!(g.objects[1].object, ObjectRef { key: 0, entry: 1 });
        assert_eq!(g.objects[1].parts[0].point_count, 1);
        assert_eq!(g.x, [10, 11, 13, 500, 20, 15]);
        assert_eq!(g.y, [0, 1023, 7, 3, 0, 0]);
        let pts: Vec<_> = g.points(&g.objects[1].parts[1]).collect();
        assert_eq!(pts, [(500, 3), (20, 0), (15, 0)]);
    }

    #[test]
    fn find_and_geoobj_records() {
        let m = GridModel::parse(&escape_model(18, None)).unwrap();
        // Find page with 3 entries: "Ab cd" (base), continuation " x" → "Ab x", empty → "Ab cd".
        let mut w = BitWriter::default();
        lit(&mut w, 3);
        for plane in [[0, 0, 0], [0, 0, 0], [0, 0, 0], [7, 7, 9]] {
            for v in plane {
                lit(&mut w, v);
            }
        }
        for f in [[30, 30, 31], [5, 5, 6], [0, 0x80, 0x80], [0, 2, 2], [4, 3, 4]] {
            for v in f {
                lit(&mut w, v);
            }
        }
        let s = b"Ab cd\0 x\0\0";
        lit(&mut w, 0);
        lit(&mut w, s.len() as u32);
        for &c in s {
            lit(&mut w, c as u32);
        }
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        let page = FindPage::decode(&mut r).unwrap();
        r.finish().unwrap();
        let keys: Vec<&[u8]> = page.entries.iter().map(|e| e.key.as_slice()).collect();
        assert_eq!(keys, [&b"Ab cd"[..], b"Ab x", b"Ab cd"]);
        assert_eq!(page.entries[2].entity, 9);

        // GeoObj with 2 entries, the second has a scale value and label fields.
        let mut w = BitWriter::default();
        lit(&mut w, 2);
        for plane in [[0, 0], [0, 0], [0, 1], [5, 0]] {
            for v in plane {
                lit(&mut w, v);
            }
        }
        lit(&mut w, 0);
        lit(&mut w, 0);
        for plane in [[0, 0], [0, 0], [0, 0], [0, 200]] {
            for v in plane {
                lit(&mut w, v);
            }
        }
        for v in [0, 0, 0, 1, 3, 4] {
            lit(&mut w, v); // e (4 byte planes), f, g
        }
        lit(&mut w, 1);
        lit(&mut w, 3);
        for c in "XA\u{4}B".encode_utf16() {
            lit(&mut w, c as u32);
        }
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        let g = GeoObjRecord::decode(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(g.entries[0].name, "X");
        assert_eq!(g.entries[1].name, "A\u{4}B");
        assert_eq!(g.entries[1].entity, 256);
        assert_eq!(g.entries[1].label, Some((1, 3, 4)));
        assert_eq!(g.entries[0].label, None);
    }
}
