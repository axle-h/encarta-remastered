//! SDF glyph PBFs for MapLibre (`glyphs/<face>/<start>-<end>.pbf`) from the disc's own TrueType
//! fonts (`AREF/FONTS`). Glyphs are rasterised at 24 px per em with a 3-px buffer and turned into
//! signed distance fields (radius 8, cutoff 0.25), the conventions MapLibre's glyph format uses.
//! A face missing from the disc falls back to the closest family on the disc (reported); a
//! character missing from a face falls back through the other disc fonts.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SIZE: f32 = 24.0;
const BUFFER: usize = 3;
const RADIUS: f64 = 8.0;
const CUTOFF: f64 = 0.25;

pub struct DiscFont {
    pub file: PathBuf,
    pub family: String,
    pub style: String,
    font: FontVec,
}

fn name_records(data: &[u8]) -> Option<(String, String)> {
    let n = u16::from_be_bytes([*data.get(4)?, *data.get(5)?]) as usize;
    let (mut off, mut len) = (None, 0usize);
    for i in 0..n {
        let r = data.get(12 + 16 * i..28 + 16 * i)?;
        if &r[0..4] == b"name" {
            off = Some(u32::from_be_bytes(r[8..12].try_into().ok()?) as usize);
            len = u32::from_be_bytes(r[12..16].try_into().ok()?) as usize;
        }
    }
    let t = data.get(off?..off? + len)?;
    let count = u16::from_be_bytes([t[2], t[3]]) as usize;
    let strings = u16::from_be_bytes([t[4], t[5]]) as usize;
    let (mut fam, mut sub) = (None, None);
    for i in 0..count {
        let r = t.get(6 + 12 * i..18 + 12 * i)?;
        let u = |k: usize| u16::from_be_bytes([r[2 * k], r[2 * k + 1]]) as usize;
        if u(0) == 3 && u(2) == 0x409 && (u(3) == 1 || u(3) == 2) {
            let s = t.get(strings + u(5)..strings + u(5) + u(4))?;
            let units: Vec<u16> = s.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            let v = String::from_utf16_lossy(&units);
            if u(3) == 1 { fam = Some(v) } else { sub = Some(v) }
        }
    }
    Some((fam?, sub?))
}

pub fn disc_fonts(dir: &Path) -> Vec<DiscFont> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf"))).collect();
    files.sort();
    for f in files {
        let Ok(data) = std::fs::read(&f) else { continue };
        let Some((family, style)) = name_records(&data) else { continue };
        let Ok(font) = FontVec::try_from_vec(data) else { continue };
        out.push(DiscFont { file: f, family, style, font });
    }
    out
}

/// Felzenszwalb–Huttenlocher 1-D squared distance transform.
fn edt_1d(f: &mut [f64], v: &mut [usize], z: &mut [f64], d: &mut [f64]) {
    let n = f.len();
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let mut k = 0usize;
    for q in 1..n {
        let mut s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * q as f64 - 2.0 * v[k] as f64);
        while s <= z[k] {
            k -= 1;
            s = ((f[q] + (q * q) as f64) - (f[v[k]] + (v[k] * v[k]) as f64)) / (2.0 * q as f64 - 2.0 * v[k] as f64);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        *dq = (q as f64 - v[k] as f64).powi(2) + f[v[k]];
    }
    f.copy_from_slice(&d[..n]);
}

fn edt_2d(grid: &mut [f64], w: usize, h: usize) {
    let m = w.max(h);
    let (mut f, mut v, mut z, mut d) = (vec![0.0; m], vec![0usize; m], vec![0.0; m + 1], vec![0.0; m]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&mut f[..h], &mut v, &mut z, &mut d);
        for y in 0..h {
            grid[y * w + x] = f[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        edt_1d(&mut f[..w], &mut v, &mut z, &mut d);
        grid[y * w..(y + 1) * w].copy_from_slice(&f[..w]);
    }
}

/// Signed distance field of a coverage bitmap (already padded), as TinySDF computes it.
pub fn sdf(coverage: &[f32], w: usize, h: usize) -> Vec<u8> {
    let inf = 1e20;
    let mut outer = vec![0.0f64; w * h];
    let mut inner = vec![0.0f64; w * h];
    for i in 0..w * h {
        let a = coverage[i] as f64;
        if a >= 1.0 {
            outer[i] = 0.0;
            inner[i] = inf;
        } else if a <= 0.0 {
            outer[i] = inf;
            inner[i] = 0.0;
        } else {
            let d = 0.5 - a;
            outer[i] = if d > 0.0 { d * d } else { 0.0 };
            inner[i] = if d < 0.0 { d * d } else { 0.0 };
        }
    }
    edt_2d(&mut outer, w, h);
    edt_2d(&mut inner, w, h);
    (0..w * h)
        .map(|i| {
            let d = outer[i].sqrt() - inner[i].sqrt();
            (255.0 - 255.0 * (d / RADIUS + CUTOFF)).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

struct Glyph {
    id: u32,
    bitmap: Vec<u8>,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    advance: u32,
}

/// A horizontal bar built into every glyph of a face (the rule between a name and its alternative,
/// mad-styling.md §7.3): rows `top…top + height` of the glyph frame (24 px per em, 0 at the line top,
/// which the PBF's `top` metric counts from the ascender), from `overhang` px before the pen position
/// to `overhang` px past the advance.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rule {
    top: f32,
    height: f32,
    overhang: f32,
}

/// Splits a rule face name `"<face> Rule <top> <height> <overhang>"` (style.rs `rule_faces`).
fn rule_face(name: &str) -> Option<(&str, Rule)> {
    let (face, spec) = name.rsplit_once(" Rule ")?;
    let v: Vec<f32> = spec.split(' ').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    match v[..] {
        [top, height, overhang] => Some((face, Rule { top, height, overhang })),
        _ => None,
    }
}

fn render_glyph(font: &FontVec, ch: char, rule: Option<Rule>) -> Option<Glyph> {
    let gid = font.glyph_id(ch);
    if gid.0 == 0 && ch != '\0' {
        return None;
    }
    let upem = font.units_per_em()?;
    let scale = PxScale::from(SIZE * font.height_unscaled() / upem);
    let sf = font.as_scaled(scale);
    let ascender = sf.ascent().round() as i32;
    let advance = sf.h_advance(gid).round().max(0.0) as u32;
    let glyph = gid.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
    let outline = font.outline_glyph(glyph);
    // Boxes in pixels from the pen position on the baseline, y down: the outline and the rule.
    let ink = outline.as_ref().map(|o| o.px_bounds()).map(|b| [b.min.x, b.min.y, b.max.x, b.max.y]);
    let bar = rule.map(|r| [-r.overhang, r.top - ascender as f32, advance as f32 + r.overhang, r.top + r.height - ascender as f32]);
    let Some(b) = [ink, bar].into_iter().flatten().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]) else {
        return Some(Glyph { id: ch as u32, bitmap: Vec::new(), width: 0, height: 0, left: 0, top: -ascender, advance });
    };
    let (x0, y0) = (b[0].floor(), b[1].floor());
    let (gw, gh) = ((b[2].ceil() - x0) as usize, (b[3].ceil() - y0) as usize);
    let (w, h) = (gw + 2 * BUFFER, gh + 2 * BUFFER);
    let mut cov = vec![0f32; w * h];
    if let (Some(o), Some(i)) = (&outline, ink) {
        let (dx, dy) = ((i[0] - x0) as usize + BUFFER, (i[1] - y0) as usize + BUFFER);
        o.draw(|x, y, c| {
            let (x, y) = (x as usize + dx, y as usize + dy);
            if x < w && y < h {
                cov[y * w + x] = c.min(1.0);
            }
        });
    }
    if let Some(r) = bar {
        // Exact area coverage of the bar per pixel.
        let overlap = |p: f32, lo: f32, hi: f32| (hi.min(p + 1.0) - lo.max(p)).max(0.0);
        for y in 0..gh {
            let fy = overlap(y0 + y as f32, r[1], r[3]);
            for x in 0..gw {
                let c = fy * overlap(x0 + x as f32, r[0], r[2]);
                let k = (y + BUFFER) * w + x + BUFFER;
                cov[k] = cov[k].max(c);
            }
        }
    }
    Some(Glyph { id: ch as u32, bitmap: sdf(&cov, w, h), width: gw as u32, height: gh as u32, left: x0 as i32, top: -(y0 as i32) - ascender, advance })
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn field_bytes(out: &mut Vec<u8>, f: u32, b: &[u8]) {
    varint(out, ((f << 3) | 2) as u64);
    varint(out, b.len() as u64);
    out.extend_from_slice(b);
}
fn field_u(out: &mut Vec<u8>, f: u32, v: u64) {
    varint(out, (f << 3) as u64);
    varint(out, v);
}
fn field_s(out: &mut Vec<u8>, f: u32, v: i32) {
    varint(out, (f << 3) as u64);
    varint(out, ((v << 1) ^ (v >> 31)) as u32 as u64);
}

fn encode_range(name: &str, start: u32, glyphs: &[Glyph]) -> Vec<u8> {
    let mut stack = Vec::new();
    field_bytes(&mut stack, 1, name.as_bytes());
    field_bytes(&mut stack, 2, format!("{}-{}", start, start + 255).as_bytes());
    for g in glyphs {
        let mut m = Vec::new();
        field_u(&mut m, 1, g.id as u64);
        if !g.bitmap.is_empty() {
            field_bytes(&mut m, 2, &g.bitmap);
        }
        field_u(&mut m, 3, g.width as u64);
        field_u(&mut m, 4, g.height as u64);
        field_s(&mut m, 5, g.left);
        field_s(&mut m, 6, g.top);
        field_u(&mut m, 7, g.advance as u64);
        field_bytes(&mut stack, 3, &m);
    }
    let mut out = Vec::new();
    field_bytes(&mut out, 1, &stack);
    out
}

#[derive(Debug, Default, Serialize)]
pub struct GlyphReport {
    pub fonts_dir: String,
    pub faces: BTreeMap<String, String>,
    pub ranges: usize,
    pub files: usize,
    pub bytes: u64,
    pub missing_chars: BTreeMap<String, usize>,
}

/// Picks the disc font for a face name "<Family> <Style>".
fn pick<'f>(fonts: &'f [DiscFont], face: &str) -> Option<(&'f DiscFont, String)> {
    let styles = ["Bold Italic", "Regular", "Bold", "Italic"];
    let (family, style) = styles.iter().find_map(|s| face.strip_suffix(&format!(" {s}")).map(|f| (f.to_string(), s.to_string())))?;
    let exact = fonts.iter().find(|f| f.family == family && f.style == style);
    if let Some(f) = exact {
        return Some((f, "exact".into()));
    }
    // Nearest style of the same family: keep the weight first (Bold Italic → Bold), then the slant.
    let prefs: &[&str] = match style.as_str() {
        "Bold Italic" => &["Bold", "Italic", "Regular"],
        "Bold" | "Italic" => &["Regular"],
        _ => &[],
    };
    let same_family = prefs
        .iter()
        .find_map(|p| fonts.iter().find(|f| f.family == family && f.style == *p))
        .or_else(|| fonts.iter().find(|f| f.family == family));
    if let Some(f) = same_family {
        return Some((f, format!("{} {} (style not on disc)", f.family, f.style)));
    }
    // Family not on the disc: a sans or serif stand-in from the disc.
    let sans = !["Times", "Georgia", "Serif", "Mincho"].iter().any(|s| family.contains(s)) || family.contains("Sans");
    let base = if sans { "MS Reference Sans Serif" } else { "MS Reference Serif" };
    fonts.iter().find(|f| f.family == base && f.style == style).or_else(|| fonts.iter().find(|f| f.family == base)).map(|f| (f, format!("{} {} (family not on disc)", f.family, f.style)))
}

/// `rule_chars`: the characters of names with an alternative, the only ones the rule faces get.
pub fn run(extracted: &Path, faces: &BTreeSet<String>, chars: &BTreeSet<char>, rule_chars: &BTreeSet<char>, out: &Path, log: &dyn Fn(String)) -> Result<GlyphReport> {
    let dir = extracted.join("iso/AREF/FONTS");
    let fonts = disc_fonts(&dir);
    let mut rep = GlyphReport { fonts_dir: dir.display().to_string(), ..Default::default() };
    if fonts.is_empty() {
        log(format!("  no TrueType fonts in {}; glyphs not generated", dir.display()));
        return Ok(rep);
    }
    let ranges_of = |chars: &BTreeSet<char>| -> BTreeSet<u32> { chars.iter().map(|&c| (c as u32) / 256 * 256).filter(|&s| s < 0x10000).collect() };
    let mut ranges = ranges_of(chars);
    ranges.insert(0);
    let rule_ranges = ranges_of(rule_chars);
    rep.ranges = ranges.len();
    // Fallback order for characters a face lacks: the large Reference faces, then the rest.
    let fallback: Vec<&DiscFont> = {
        let mut v: Vec<&DiscFont> = fonts.iter().filter(|f| f.family.starts_with("MS Reference") && f.style == "Regular").collect();
        v.extend(fonts.iter().filter(|f| f.style == "Regular" && !f.family.starts_with("MS Reference")));
        v
    };
    let mut faces: Vec<String> = faces.iter().cloned().collect();
    // Always provide a default face for clients and placeholder layers.
    for f in ["Arial Regular", "Arial Bold"] {
        if !faces.iter().any(|x| x == f) {
            faces.push(f.into());
        }
    }
    for face in &faces {
        let (base, rule) = match rule_face(face) {
            Some((b, r)) => (b, Some(r)),
            None => (face.as_str(), None),
        };
        let Some((font, how)) = pick(&fonts, base) else { continue };
        let (chars, ranges) = if rule.is_some() { (rule_chars, &rule_ranges) } else { (chars, &ranges) };
        rep.faces.insert(face.clone(), format!("{} ← {}", how, font.file.file_name().unwrap().to_string_lossy()));
        let face_dir = out.join("glyphs").join(face);
        std::fs::create_dir_all(&face_dir)?;
        let results: Vec<(usize, u64, usize)> = ranges
            .par_iter()
            .map(|&start| -> Result<(usize, u64, usize)> {
                let mut glyphs = Vec::new();
                let mut missing = 0;
                for cp in start..start + 256 {
                    let Some(ch) = char::from_u32(cp) else { continue };
                    let needed = chars.contains(&ch) || (cp < 256 && rule.is_none());
                    if rule.is_some() && !needed {
                        continue;
                    }
                    let g = render_glyph(&font.font, ch, rule).or_else(|| if needed { fallback.iter().find_map(|f| render_glyph(&f.font, ch, rule)) } else { None });
                    match g {
                        Some(g) => glyphs.push(g),
                        None if chars.contains(&ch) => missing += 1,
                        None => {}
                    }
                }
                let bytes = encode_range(face, start, &glyphs);
                let p = face_dir.join(format!("{}-{}.pbf", start, start + 255));
                std::fs::write(&p, &bytes).with_context(|| format!("writing {}", p.display()))?;
                Ok((1, bytes.len() as u64, missing))
            })
            .collect::<Result<_>>()?;
        for (n, b, m) in results {
            rep.files += n;
            rep.bytes += b;
            if m > 0 {
                *rep.missing_chars.entry(face.clone()).or_default() += m;
            }
        }
    }
    log(format!("  glyphs: {} faces × {} ranges, {:.1} MB", rep.faces.len(), rep.ranges, rep.bytes as f64 / 1e6));
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_face_names() {
        assert_eq!(rule_face("Arial Bold Rule 24.00 2.00 1.50"), Some(("Arial Bold", Rule { top: 24.0, height: 2.0, overhang: 1.5 })));
        assert_eq!(rule_face("Arial Bold Rule -5.60 2.33 0.00").map(|r| r.1.top), Some(-5.6));
        assert_eq!(rule_face("Arial Bold"), None);
        assert_eq!(rule_face("Arial Rule x"), None);
    }

    /// With the disc's fonts (skipped without `extracted/`): a ruled space is all bar, and a ruled
    /// letter keeps its ink and spans its advance plus the overhangs.
    #[test]
    fn ruled_glyphs_from_disc() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/iso/AREF/FONTS");
        let fonts = disc_fonts(&dir);
        let Some(f) = fonts.iter().find(|f| f.family == "Arial" && f.style == "Regular") else {
            eprintln!("skipped: no disc fonts at {}", dir.display());
            return;
        };
        let rule = Rule { top: 24.0, height: 2.0, overhang: 1.0 };
        let space = render_glyph(&f.font, ' ', Some(rule)).unwrap();
        assert_eq!((space.left, space.width as i32, space.height), (-1, space.advance as i32 + 2, 2));
        let plain = render_glyph(&f.font, 'M', None).unwrap();
        let m = render_glyph(&f.font, 'M', Some(rule)).unwrap();
        assert_eq!((m.top, m.advance), (plain.top, plain.advance));
        assert!(m.height > plain.height && m.left <= plain.left.min(-1));
    }

    #[test]
    fn sdf_of_a_square() {
        // A filled 10×10 square inside a 20×20 field: inside > 191 (edge value), outside < 191.
        let (w, h) = (20, 20);
        let cov: Vec<f32> = (0..w * h).map(|i| if (5..15).contains(&(i % w)) && (5..15).contains(&(i / w)) { 1.0 } else { 0.0 }).collect();
        let s = sdf(&cov, w, h);
        let edge = (255.0 * (1.0 - CUTOFF)) as u8;
        assert!(s[10 * w + 10] > edge);
        assert!(s[0] < edge / 2);
        assert!(s[10 * w + 10] >= s[10 * w + 6] && s[10 * w + 6] >= s[10 * w + 4]);
    }

    #[test]
    fn glyph_pbf_layout() {
        let g = Glyph { id: 65, bitmap: vec![1, 2, 3], width: 1, height: 1, left: -1, top: -5, advance: 7 };
        let b = encode_range("Test Regular", 0, &[g]);
        // fontstack (field 1, length-delimited) wrapping name "Test Regular" and range "0-255".
        assert_eq!(b[0], 0x0A);
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("Test Regular") && s.contains("0-255"));
    }
}
