//! Atlas media resolution: where the MSWORLD.ITR photos and sounds (`atlas_media` in
//! `content.sqlite`, msworld.md §3) really are on the disc. Their catalog names (`14638.jpg`,
//! `t090206a.wav`) are mostly not disc file names, so each one is looked for, in order:
//!
//! 1. `catalog`: the name itself (catalog key or file name, as `/media/file/` resolves it);
//! 2. `same-id`: a DATA item with the record's own id, and that item's image or audio file;
//! 3. `ext-variant`: the same stem with another extension of the same kind (`.wav` → `.wma`);
//! 4. `credit` (sounds): a DATA audio item whose credit names the same recording (quoted track
//!    title and catalogue number);
//! 5. `thumbnail` (pictures): the atlas pushpin (`MSWORLD.EIT/dtn/<stem>.dtn`, 64×64) is
//!    pixel-for-pixel a DATA item's 64-px thumbnail; the picture is that item's image;
//! 6. `crop` (pictures): the pushpin is a square crop of the image of a DATA item with the same
//!    credit, photographer or title (normalised cross-correlation over crops, docs/media.md §7).
//!
//! The result is the manifest table `atlas_file` (one row per catalog file name; `sha256` NULL =
//! not on the disc), which the server consults after the catalog and file names, and the report
//! `out/report/atlas-media.json`. Runs as `encarta-convert atlas-media` and at the end of `media`.

use anyhow::{Context, Result};
use image::RgbImage;
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags, params};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS atlas_file(
    name       TEXT PRIMARY KEY,
    role       TEXT NOT NULL,
    ref_id     INTEGER NOT NULL,
    records    INTEGER NOT NULL,
    linked     INTEGER NOT NULL,
    sha256     TEXT,
    resolution TEXT NOT NULL,
    via        TEXT,
    score      REAL
);
";

// ---- thresholds (calibrated on the 3,294 atlas pictures whose catalog name is on the disc) ----

/// Thumbnail stage: RMS difference (0–255) of 16×16 RGB averages; true pairs measure 3–8.
const THUMB_MAX_RMS: f64 = 10.0;
/// …and the next-best thumbnail of another picture must be at least this far.
const THUMB_MIN_SECOND: f64 = 20.0;
/// Crop stage: grey 16×16 correlation of the best crop, colour 32×32 correlation there,
/// crop side as a fraction of the image's short side, contrast ratio crop / pushpin.
const CROP_MIN_GREY: f64 = 0.93;
const CROP_MIN_COLOUR: f64 = 0.85;
const CROP_MIN_FRAC: f64 = 0.33;
const CROP_CONTRAST: (f64, f64) = (0.7, 1.4);
/// Another candidate within this much of the best grey score makes the match ambiguous, unless
/// its crop shows the same picture (correlation of the two crops ≥ `CROP_SAME_PICTURE`).
const CROP_AMBIGUOUS: f64 = 0.03;
const CROP_SAME_PICTURE: f64 = 0.9;
const CROP_SEARCH_MIN_FRAC: f64 = 0.3;

// ---- images: integral planes and crop sampling ----

/// Summed-area tables of an RGB image: grey (R+G+B) and each channel, `(w+1)×(h+1)`.
pub struct Planes {
    pub w: usize,
    pub h: usize,
    grey: Vec<u64>,
    rgb: [Vec<u64>; 3],
}

impl Planes {
    pub fn new(img: &RgbImage) -> Planes {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let stride = w + 1;
        let mut grey = vec![0u64; stride * (h + 1)];
        let mut rgb = [grey.clone(), grey.clone(), grey.clone()];
        for y in 0..h {
            let mut row = [0u64; 4];
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32).0;
                for c in 0..3 {
                    row[c] += p[c] as u64;
                }
                row[3] += p[0] as u64 + p[1] as u64 + p[2] as u64;
                let i = (y + 1) * stride + x + 1;
                let up = y * stride + x + 1;
                for c in 0..3 {
                    rgb[c][i] = rgb[c][up] + row[c];
                }
                grey[i] = grey[up] + row[3];
            }
        }
        Planes { w, h, grey, rgb }
    }

    fn rect(t: &[u64], stride: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        (t[y1 * stride + x1] + t[y0 * stride + x0] - t[y0 * stride + x1] - t[y1 * stride + x0]) as f64
    }

    /// Means of an `n×n` grid over the rectangle (x, y, rw, rh); cell edges are rounded.
    #[allow(clippy::too_many_arguments)]
    fn grid(&self, t: &[u64], x: usize, y: usize, rw: usize, rh: usize, n: usize, out: &mut [f64]) {
        let stride = self.w + 1;
        let ex = |i: usize| x + ((i * rw) as f64 / n as f64).round() as usize;
        let ey = |i: usize| y + ((i * rh) as f64 / n as f64).round() as usize;
        for j in 0..n {
            let (y0, y1) = (ey(j), ey(j + 1));
            for i in 0..n {
                let (x0, x1) = (ex(i), ex(i + 1));
                let area = ((x1 - x0) * (y1 - y0)).max(1) as f64;
                out[j * n + i] = Self::rect(t, stride, x0, y0, x1, y1) / area;
            }
        }
    }

    pub fn grey_grid(&self, x: usize, y: usize, rw: usize, rh: usize, n: usize) -> Vec<f64> {
        let mut v = vec![0.0; n * n];
        self.grid(&self.grey, x, y, rw, rh, n, &mut v);
        v
    }

    /// `n×n` RGB means, channel-major (all R, then G, then B).
    pub fn rgb_grid(&self, x: usize, y: usize, rw: usize, rh: usize, n: usize) -> Vec<f64> {
        let mut v = vec![0.0; 3 * n * n];
        for c in 0..3 {
            self.grid(&self.rgb[c], x, y, rw, rh, n, &mut v[c * n * n..(c + 1) * n * n]);
        }
        v
    }
}

fn mean_std(v: &[f64]) -> (f64, f64) {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64;
    (m, var.sqrt())
}

/// Zero-mean, unit-length copy (all zeros for a flat vector).
fn normalised(v: &[f64]) -> Vec<f64> {
    let (m, _) = mean_std(v);
    let mut o: Vec<f64> = v.iter().map(|x| x - m).collect();
    let n = o.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n > 1e-9 {
        o.iter_mut().for_each(|x| *x /= n);
    }
    o
}

/// Normalised cross-correlation of two equal-length vectors.
#[cfg(test)]
fn ncc(a: &[f64], b: &[f64]) -> f64 {
    normalised(a).iter().zip(normalised(b)).map(|(x, y)| x * y).sum()
}

/// Colour correlation: each channel's mean removed, then the cosine of the whole vector.
fn ncc_rgb(a: &[f64], b: &[f64]) -> f64 {
    let centre = |v: &[f64]| -> Vec<f64> {
        let k = v.len() / 3;
        let mut o = v.to_vec();
        for c in 0..3 {
            let m = v[c * k..(c + 1) * k].iter().sum::<f64>() / k as f64;
            o[c * k..(c + 1) * k].iter_mut().for_each(|x| *x -= m);
        }
        o
    };
    let (a, b) = (centre(a), centre(b));
    let d: f64 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let n = (a.iter().map(|x| x * x).sum::<f64>() * b.iter().map(|x| x * x).sum::<f64>()).sqrt();
    if n > 1e-9 { d / n } else { 0.0 }
}

/// A 64-px pushpin prepared for matching.
pub struct Query {
    grey16: Vec<f64>,
    grey16_std: f64,
    rgb32: Vec<f64>,
}

impl Query {
    pub fn new(img: &RgbImage) -> Query {
        let p = Planes::new(img);
        let g = p.grey_grid(0, 0, p.w, p.h, 16);
        Query { grey16_std: mean_std(&g).1, grey16: normalised(&g), rgb32: p.rgb_grid(0, 0, p.w, p.h, 32) }
    }
}

#[derive(Debug, Clone)]
pub struct CropMatch {
    pub grey: f64,
    pub colour: f64,
    pub frac: f64,
    pub contrast: f64,
    pub bbox: (usize, usize, usize),
    /// The best crop's normalised 16×16 grey grid (to tell two candidates' pictures apart).
    pub crop: Vec<f64>,
}

impl CropMatch {
    pub fn accepted(&self) -> bool {
        self.grey >= CROP_MIN_GREY
            && self.colour >= CROP_MIN_COLOUR
            && self.frac >= CROP_MIN_FRAC
            && (CROP_CONTRAST.0..=CROP_CONTRAST.1).contains(&self.contrast)
    }
}

/// Grey correlation of the square crop (x, y, side) with the query, or `None` outside the image.
fn crop_score(p: &Planes, q: &Query, x: usize, y: usize, side: usize, buf: &mut [f64; 256]) -> Option<f64> {
    if side < 16 || x + side > p.w || y + side > p.h {
        return None;
    }
    p.grid(&p.grey, x, y, side, side, 16, buf);
    let m = buf.iter().sum::<f64>() / 256.0;
    let mut dot = 0.0;
    let mut ss = 0.0;
    for (v, w) in buf.iter().zip(&q.grey16) {
        dot += v * w;
        ss += (v - m) * (v - m);
    }
    Some(if ss > 1e-9 { dot / ss.sqrt() } else { 0.0 })
}

type Best = Option<(f64, usize, usize, usize)>;

fn consider(best: &mut Best, s: Option<f64>, x: usize, y: usize, side: usize) {
    if let Some(s) = s
        && best.is_none_or(|b| s > b.0)
    {
        *best = Some((s, x, y, side));
    }
}

/// Best square crop of `p` matching the pushpin: a coarse search over ten crop sizes from 30 %
/// to 100 % of the short side (steps of a tenth of the crop), then two refinements.
pub fn best_crop(p: &Planes, q: &Query) -> Option<CropMatch> {
    let short = p.w.min(p.h);
    if short < 32 {
        return None;
    }
    let mut buf = [0.0f64; 256];
    let mut best: Best = None;
    for k in 0..10 {
        let frac = CROP_SEARCH_MIN_FRAC * (1.0 / CROP_SEARCH_MIN_FRAC).powf(k as f64 / 9.0);
        let side = ((short as f64 * frac) as usize).max(16);
        let step = (side / 10).max(1);
        let mut y = 0;
        while y + side <= p.h {
            let mut x = 0;
            while x + side <= p.w {
                consider(&mut best, crop_score(p, q, x, y, side, &mut buf), x, y, side);
                x += step;
            }
            y += step;
        }
    }
    let (_, mut x0, mut y0, mut side0) = best?;
    for _ in 0..2 {
        let st = (side0 / 20).max(1) as isize;
        for ds in -2isize..=2 {
            for dx in -2isize..=2 {
                for dy in -2isize..=2 {
                    let side = side0 as isize + ds * st;
                    let (x, y) = (x0 as isize + dx * st, y0 as isize + dy * st);
                    if side < 16 || x < 0 || y < 0 {
                        continue;
                    }
                    consider(&mut best, crop_score(p, q, x as usize, y as usize, side as usize, &mut buf), x as usize, y as usize, side as usize);
                }
            }
        }
        (_, x0, y0, side0) = best?;
    }
    let (grey, x, y, side) = best?;
    let g = p.grey_grid(x, y, side, side, 16);
    let contrast = if q.grey16_std > 1e-9 { mean_std(&g).1 / q.grey16_std } else { 0.0 };
    Some(CropMatch {
        grey,
        colour: ncc_rgb(&p.rgb_grid(x, y, side, side, 32), &q.rgb32),
        frac: side as f64 / short as f64,
        contrast,
        bbox: (x, y, side),
        crop: normalised(&g),
    })
}

/// 16×16 RGB averages of a whole (thumbnail-size) image, for the thumbnail stage.
pub fn thumb_feature(img: &RgbImage) -> Vec<f32> {
    let p = Planes::new(img);
    p.rgb_grid(0, 0, p.w, p.h, 16).into_iter().map(|v| v as f32).collect()
}

fn rms(a: &[f32], b: &[f32]) -> f64 {
    let s: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
    (s as f64 / a.len() as f64).sqrt()
}

/// Among several candidates' crop matches (any order), the accepted one, or why none is.
pub fn pick_crop<T>(cands: &[(T, CropMatch)]) -> std::result::Result<&(T, CropMatch), &'static str> {
    let Some(best) = cands.iter().max_by(|a, b| a.1.grey.total_cmp(&b.1.grey)) else { return Err("no candidate") };
    if !best.1.accepted() {
        return Err("below threshold");
    }
    let rival = cands.iter().any(|c| {
        !std::ptr::eq(c, best)
            && c.1.grey >= best.1.grey - CROP_AMBIGUOUS
            && c.1.crop.iter().zip(&best.1.crop).map(|(a, b)| a * b).sum::<f64>() < CROP_SAME_PICTURE
    });
    if rival { Err("ambiguous") } else { Ok(best) }
}

// ---- metadata keys ----

/// Lower-case ASCII letters and digits only.
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// The photographer of a picture credit ("Paul A. Souders/Corbis" → "paulasouders").
fn photographer(credit: &str) -> String {
    norm(credit.split('/').next().unwrap_or(""))
}

/// A recording's identity from a sound credit: its first quoted title and the digits of its
/// catalogue number (`"Dance Tune" … (Cat.# LDX 274 752)` → ("dancetune", "274752")).
pub fn recording_key(credit: &str) -> Option<(String, String)> {
    let is_q = |c: char| matches!(c, '"' | '\u{201C}' | '\u{201D}');
    let start = credit.find(is_q)?;
    let rest = &credit[start..];
    let rest = &rest[rest.chars().next()?.len_utf8()..];
    let title = norm(&rest[..rest.find(is_q)?]);
    let cat = credit.find("Cat.")?;
    let after = credit[cat + 4..].trim_start().strip_prefix('#')?;
    let digits: String = after.split(')').next()?.chars().filter(char::is_ascii_digit).collect();
    (!title.is_empty() && !digits.is_empty()).then_some((title, digits))
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(a, _)| a)
}

// ---- the disc (manifest) and the DATA records (content.sqlite) ----

struct Disc {
    /// catalog key → shas (deluxe first), file name → shas (non-Kids containers first).
    catalog: HashMap<String, Vec<String>>,
    files: HashMap<String, Vec<String>>,
    names_of: HashMap<String, Vec<String>>,
    original: HashMap<String, String>,
}

impl Disc {
    fn load(m: &Connection) -> Result<Disc> {
        let mut catalog: HashMap<String, Vec<String>> = HashMap::new();
        let mut st = m.prepare(
            "SELECT key, sha256 FROM catalog_key WHERE edition IN ('deluxe','kids') AND tbl IN ('baggage','generic')
             AND sha256 IS NOT NULL ORDER BY key, edition = 'deluxe' DESC",
        )?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (k, s) = r?;
            catalog.entry(k.to_ascii_lowercase()).or_default().push(s);
        }
        let mut files: HashMap<String, Vec<String>> = HashMap::new();
        let mut names_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut st = m.prepare("SELECT name, sha256 FROM file ORDER BY name, container LIKE 'EE/KIDS/%', container, member_path")?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (n, s) = r?;
            let v = names_of.entry(s.clone()).or_default();
            if !v.contains(&n) {
                v.push(n.clone());
            }
            files.entry(n).or_default().push(s);
        }
        let mut original = HashMap::new();
        let mut st = m.prepare("SELECT sha256, path FROM derivative WHERE format = 'original'")?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (s, p) = r?;
            original.insert(s, p);
        }
        Ok(Disc { catalog, files, names_of, original })
    }

    /// sha256 of a disc name, as the server's `/media/file/{name}` resolves it.
    fn resolve(&self, name: &str) -> Option<&str> {
        let n = name.to_ascii_lowercase();
        self.catalog.get(&n).or_else(|| self.files.get(&n)).and_then(|v| v.first()).map(String::as_str)
    }
}

#[derive(Default)]
struct Item {
    title: String,
    credit: String,
    files: Vec<(String, String)>,
}

impl Item {
    fn file(&self, role: &str) -> Option<&str> {
        self.files.iter().find(|(r, _)| r == role).map(|(_, f)| f.as_str())
    }
}

struct Record {
    ref_id: i64,
    kind: String,
    title: String,
    file: Option<String>,
    thumbnail: Option<String>,
    credit: String,
    linked: bool,
}

fn load_content(c: &Connection) -> Result<(Vec<Record>, BTreeMap<i64, Item>)> {
    let linked: HashSet<i64> = {
        let mut st = c.prepare("SELECT DISTINCT content_ref_id FROM place_link")?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    let mut st = c.prepare("SELECT ref_id, kind, title, file, thumbnail, credit FROM atlas_media ORDER BY ref_id")?;
    let recs = st
        .query_map([], |r| {
            Ok(Record {
                ref_id: r.get(0)?,
                kind: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                file: r.get(3)?,
                thumbnail: r.get(4)?,
                credit: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                linked: false,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|mut r| {
            r.linked = linked.contains(&r.ref_id);
            r
        })
        .collect();
    let mut items: BTreeMap<i64, Item> = BTreeMap::new();
    let mut st = c.prepare(
        "SELECT ref_id, role, file_name FROM item_file WHERE role IN ('image','audio','thumb','picon') AND file_name IS NOT NULL ORDER BY ref_id, ordinal",
    )?;
    for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
        let (id, role, f) = r?;
        items.entry(id).or_default().files.push((role, f.to_ascii_lowercase()));
    }
    let mut st = c.prepare("SELECT ref_id, title, credit FROM item WHERE ref_id IN (SELECT DISTINCT ref_id FROM item_file WHERE role IN ('image','audio'))")?;
    for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)))? {
        let (id, t, cr) = r?;
        if let Some(it) = items.get_mut(&id) {
            it.title = t.unwrap_or_default();
            it.credit = cr.unwrap_or_default();
        }
    }
    Ok((recs, items))
}

#[derive(Clone, Debug)]
struct Found {
    how: &'static str,
    sha: String,
    via: String,
    score: Option<f64>,
    /// The DATA item the picture came from (for its picon).
    item: Option<i64>,
}

fn load_rgb(disc: &Disc, media_dir: &Path, sha: &str) -> Option<RgbImage> {
    let p = disc.original.get(sha)?;
    let bytes = std::fs::read(media_dir.join(p)).ok()?;
    image::load_from_memory(&bytes).ok().map(|i| i.to_rgb8())
}

/// Run the resolution and write `atlas_file` + the report. `content` is `out/content.sqlite`.
pub fn run(content: &Path, media_dir: &Path, report_dir: &Path, log: &dyn Fn(String)) -> Result<()> {
    let t0 = std::time::Instant::now();
    let c = Connection::open_with_flags(content, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| format!("opening {}", content.display()))?;
    let has_table = |t: &str| -> rusqlite::Result<bool> {
        c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)", [t], |r| r.get(0))
    };
    if !has_table("atlas_media")? {
        log("atlas media: content.sqlite has no atlas_media table, skipped".into());
        return Ok(());
    }
    let (recs, items) = load_content(&c)?;
    let mut db = Connection::open(media_dir.join("manifest.sqlite"))?;
    db.execute_batch(SCHEMA)?;
    let disc = Disc::load(&db)?;
    log(format!("atlas media: {} records, {} DATA items with files ({:.1} s)", recs.len(), items.len(), t0.elapsed().as_secs_f64()));

    let mut found: Vec<Option<Found>> = vec![None; recs.len()];
    let mut why: Vec<&'static str> = vec!["absent"; recs.len()];
    let audio = |r: &Record| r.kind == "w";
    let sound_exts: &[&str] = &["wma", "wav", "mp3", "asf"];
    let picture_exts: &[&str] = &["jpg", "gif", "png", "bmp", "dib"];

    // Stages 1–3: names.
    for (i, r) in recs.iter().enumerate() {
        let Some(file) = r.file.as_deref() else { continue };
        if let Some(s) = disc.resolve(file) {
            found[i] = Some(Found { how: "catalog", sha: s.into(), via: file.into(), score: None, item: None });
            continue;
        }
        let role = if audio(r) { "audio" } else { "image" };
        if let Some(it) = items.get(&r.ref_id)
            && let Some(f) = it.file(role)
            && let Some(s) = disc.resolve(f)
        {
            found[i] = Some(Found { how: "same-id", sha: s.into(), via: f.into(), score: None, item: Some(r.ref_id) });
            continue;
        }
        let exts = if audio(r) { sound_exts } else { picture_exts };
        for e in exts {
            let alt = format!("{}.{e}", stem(file));
            if alt != file
                && let Some(s) = disc.resolve(&alt)
            {
                found[i] = Some(Found { how: "ext-variant", sha: s.into(), via: alt, score: None, item: None });
                break;
            }
        }
    }

    // Stage 4: sounds by recording.
    let mut by_recording: HashMap<(String, String), Vec<(i64, String)>> = HashMap::new();
    for (&id, it) in &items {
        if let (Some(k), Some(f)) = (recording_key(&it.credit), it.file("audio"))
            && disc.resolve(f).is_some()
        {
            by_recording.entry(k).or_default().push((id, f.to_string()));
        }
    }
    for (i, r) in recs.iter().enumerate() {
        if found[i].is_some() || !audio(r) {
            continue;
        }
        let Some(hits) = recording_key(&r.credit).and_then(|k| by_recording.get(&k)) else { continue };
        let shas: HashSet<&str> = hits.iter().filter_map(|(_, f)| disc.resolve(f)).collect();
        if shas.len() == 1 {
            let (id, f) = &hits[0];
            found[i] = Some(Found { how: "credit", sha: disc.resolve(f).unwrap().into(), via: f.clone(), score: None, item: Some(*id) });
        } else {
            why[i] = "ambiguous";
        }
    }

    // Pushpins of the pictures still missing.
    let pin_of = |r: &Record| -> Option<String> {
        let s = r.thumbnail.as_deref().or(r.file.as_deref()).map(stem)?;
        disc.files.get(&format!("{s}.dtn")).and_then(|v| v.first()).cloned()
    };
    let pending: Vec<(usize, String)> =
        recs.iter().enumerate().filter(|(i, r)| found[*i].is_none() && !audio(r)).filter_map(|(i, r)| pin_of(r).map(|p| (i, p))).collect();
    let pin_shas: Vec<String> = pending.iter().map(|(_, p)| p.clone()).collect::<HashSet<_>>().into_iter().collect();
    let pins: HashMap<String, RgbImage> =
        pin_shas.par_iter().filter_map(|s| load_rgb(&disc, media_dir, s).map(|im| (s.clone(), im))).collect();
    log(format!("atlas media: {} pictures to match ({} pushpins)", pending.len(), pins.len()));

    // Stage 5: thumbnails. Gallery = every 64-px thumbnail (`.jtn`/`.gtn`) on the disc.
    let thumb_items: HashMap<&str, Vec<i64>> = {
        let mut m: HashMap<&str, Vec<i64>> = HashMap::new();
        for (&id, it) in &items {
            if let Some(t) = it.file("thumb") {
                m.entry(t).or_default().push(id);
            }
        }
        m
    };
    let gallery_shas: Vec<String> = {
        let mut v: Vec<String> = disc
            .files
            .iter()
            .filter(|(n, _)| n.ends_with(".jtn") || n.ends_with(".gtn"))
            .flat_map(|(_, s)| s.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        v.sort();
        v
    };
    let gallery: Vec<(String, Vec<f32>)> =
        gallery_shas.par_iter().filter_map(|s| load_rgb(&disc, media_dir, s).map(|im| (s.clone(), thumb_feature(&im)))).collect();
    // The image a thumbnail stands for: its DATA items' image, else the same stem as `.jpg`/`.gif`.
    let image_of_thumb = |sha: &str| -> Option<(String, Option<i64>)> {
        let names = disc.names_of.get(sha)?;
        for n in names {
            for &id in thumb_items.get(n.as_str()).into_iter().flatten() {
                if let Some(f) = items[&id].file("image")
                    && disc.resolve(f).is_some()
                {
                    return Some((f.to_string(), Some(id)));
                }
            }
        }
        names.iter().flat_map(|n| ["jpg", "gif"].map(|e| format!("{}.{e}", stem(n)))).find(|n| disc.resolve(n).is_some()).map(|n| (n, None))
    };
    let thumb_hits: Vec<Option<(String, f64, f64)>> = pending
        .par_iter()
        .map(|(_, pin)| {
            let q = thumb_feature(pins.get(pin)?);
            let mut best: Vec<(f64, &str)> = gallery.iter().map(|(s, f)| (rms(&q, f), s.as_str())).collect();
            best.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (d, s) = *best.first()?;
            if d > THUMB_MAX_RMS {
                return None;
            }
            let target = image_of_thumb(s).map(|t| t.0);
            // The next-best thumbnail of a different picture.
            let second = best[1..].iter().find(|(_, s2)| image_of_thumb(s2).map(|t| t.0) != target || target.is_none()).map_or(f64::MAX, |x| x.0);
            Some((s.to_string(), d, second))
        })
        .collect();
    for ((i, _), hit) in pending.iter().zip(&thumb_hits) {
        let Some((sha, d, second)) = hit else { continue };
        if *second < THUMB_MIN_SECOND {
            why[*i] = "ambiguous";
            continue;
        }
        if let Some((f, item)) = image_of_thumb(sha) {
            found[*i] = Some(Found { how: "thumbnail", sha: disc.resolve(&f).unwrap().into(), via: f, score: Some(*d), item });
        }
    }

    // Stage 6: crops of candidate pictures (same credit, photographer or title).
    let mut by_credit: HashMap<String, Vec<i64>> = HashMap::new();
    let mut by_phot: HashMap<String, Vec<i64>> = HashMap::new();
    let mut by_title: HashMap<String, Vec<i64>> = HashMap::new();
    for (&id, it) in &items {
        if it.file("image").and_then(|f| disc.resolve(f)).is_none() {
            continue;
        }
        let (c, p, t) = (norm(&it.credit), photographer(&it.credit), norm(&it.title));
        if !c.is_empty() {
            by_credit.entry(c).or_default().push(id);
        }
        if p.len() >= 4 {
            by_phot.entry(p).or_default().push(id);
        }
        if !t.is_empty() {
            by_title.entry(t).or_default().push(id);
        }
    }
    // query index → candidate (sha → (image name, item)); grouped by sha so each image decodes once.
    let mut cands: Vec<BTreeMap<String, (String, i64)>> = vec![BTreeMap::new(); pending.len()];
    let mut by_sha: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (qi, (i, _)) in pending.iter().enumerate() {
        if found[*i].is_some() {
            continue;
        }
        let r = &recs[*i];
        let keys = [(&by_credit, norm(&r.credit)), (&by_phot, photographer(&r.credit)), (&by_title, norm(&r.title))];
        for (map, key) in keys {
            if key.is_empty() {
                continue;
            }
            for &id in map.get(&key).into_iter().flatten() {
                let f = items[&id].file("image").unwrap();
                let s = disc.resolve(f).unwrap().to_string();
                cands[qi].entry(s).or_insert_with(|| (f.to_string(), id));
            }
        }
        for s in cands[qi].keys() {
            by_sha.entry(s.clone()).or_default().push(qi);
        }
    }
    let pairs: usize = by_sha.values().map(Vec::len).sum();
    log(format!("atlas media: crop search, {} candidate pictures, {pairs} pairs", by_sha.len()));
    let queries: Vec<Option<Query>> = pending.iter().map(|(_, p)| pins.get(p).map(Query::new)).collect();
    let mut results: Vec<(usize, String, CropMatch)> = by_sha
        .par_iter()
        .flat_map_iter(|(sha, qs)| {
            let planes = load_rgb(&disc, media_dir, sha).map(|im| Planes::new(&im));
            qs.iter()
                .filter_map(|&qi| best_crop(planes.as_ref()?, queries[qi].as_ref()?).map(|m| (qi, sha.clone(), m)))
                .collect::<Vec<_>>()
        })
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut per_query: BTreeMap<usize, Vec<(String, CropMatch)>> = BTreeMap::new();
    for (qi, sha, m) in results {
        per_query.entry(qi).or_default().push((sha, m));
    }
    let mut crop_log = Vec::new();
    for (qi, list) in &per_query {
        let i = pending[*qi].0;
        match pick_crop(list) {
            Ok((sha, m)) => {
                let (f, id) = cands[*qi][sha].clone();
                crop_log.push(json!({ "ref_id": recs[i].ref_id, "title": recs[i].title, "via": f, "grey": m.grey, "colour": m.colour,
                    "frac": m.frac, "contrast": m.contrast, "box": [m.bbox.0, m.bbox.1, m.bbox.2] }));
                found[i] = Some(Found { how: "crop", sha: sha.clone(), via: f, score: Some(m.grey), item: Some(id) });
            }
            Err("ambiguous") => why[i] = "ambiguous",
            Err(_) => {}
        }
    }

    // Rows: one per catalog file name; the picon follows the picture's DATA item.
    struct Row {
        role: &'static str,
        ref_id: i64,
        records: i64,
        linked: bool,
        sha: Option<String>,
        how: &'static str,
        via: Option<String>,
        score: Option<f64>,
    }
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    let mut put = |name: &str, role: &'static str, r: &Record, f: Option<(&'static str, String, String, Option<f64>)>, why: &'static str| {
        let row = rows.entry(name.to_string()).or_insert(Row { role, ref_id: r.ref_id, records: 0, linked: false, sha: None, how: why, via: None, score: None });
        row.records += 1;
        row.linked |= r.linked;
        if row.sha.is_none()
            && let Some((how, sha, via, score)) = f
        {
            (row.sha, row.how, row.via, row.score) = (Some(sha), how, Some(via), score);
        }
    };
    for (i, r) in recs.iter().enumerate() {
        let role = if audio(r) { "audio" } else { "image" };
        if let Some(file) = r.file.as_deref() {
            put(file, role, r, found[i].clone().map(|f| (f.how, f.sha, f.via, f.score)), why[i]);
        }
        if let Some(p) = r.thumbnail.as_deref() {
            let pf = if let Some(s) = disc.resolve(p) {
                Some(("catalog", s.to_string(), p.to_string(), None))
            } else {
                found[i].as_ref().and_then(|f| {
                    let pic = f.item.and_then(|id| items[&id].file("picon"))?;
                    disc.resolve(pic).map(|s| (f.how, s.to_string(), pic.to_string(), f.score))
                })
            };
            put(p, "picon", r, pf, why[i]);
        }
    }

    {
        let tx = db.transaction()?;
        tx.execute("DELETE FROM atlas_file", [])?;
        {
            let mut st = tx.prepare("INSERT INTO atlas_file(name, role, ref_id, records, linked, sha256, resolution, via, score) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")?;
            for (n, r) in &rows {
                st.execute(params![n, r.role, r.ref_id, r.records, r.linked, r.sha, r.how, r.via, r.score])?;
            }
        }
        tx.commit()?;
    }

    // Report: records and file names by kind × resolution, linked ones (P2MTBL), what is absent.
    let mut by_record: BTreeMap<String, BTreeMap<String, [u64; 2]>> = BTreeMap::new();
    let mut absent = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        let k = match r.kind.as_str() {
            "w" => "sound",
            "m" => "animal picture",
            _ => "picture",
        };
        let how = found[i].as_ref().map_or(why[i], |f| f.how);
        let e = by_record.entry(k.into()).or_default().entry(how.into()).or_default();
        e[0] += 1;
        if r.linked {
            e[1] += 1;
        }
        if found[i].is_none() {
            absent.push(json!({ "ref_id": r.ref_id, "kind": k, "title": r.title, "file": r.file, "linked": r.linked,
                "data_item": items.contains_key(&r.ref_id), "reason": why[i] }));
        }
    }
    let records: serde_json::Value = by_record
        .iter()
        .map(|(k, m)| (k.clone(), m.iter().map(|(h, [n, l])| (h.clone(), json!({ "records": n, "linked": l }))).collect::<serde_json::Map<_, _>>().into()))
        .collect::<serde_json::Map<_, _>>()
        .into();
    let mut by_name: BTreeMap<String, u64> = BTreeMap::new();
    for r in rows.values() {
        *by_name.entry(format!("{}:{}", r.role, r.how)).or_default() += 1;
    }
    let report = json!({
        "elapsed_s": t0.elapsed().as_secs_f64(),
        "thresholds": { "thumb_max_rms": THUMB_MAX_RMS, "thumb_min_second": THUMB_MIN_SECOND, "crop_min_grey": CROP_MIN_GREY,
            "crop_min_colour": CROP_MIN_COLOUR, "crop_min_frac": CROP_MIN_FRAC, "crop_contrast": CROP_CONTRAST,
            "crop_ambiguous": CROP_AMBIGUOUS, "crop_same_picture": CROP_SAME_PICTURE },
        "records_by_resolution": records,
        "file_names_by_role_resolution": by_name,
        "crop_pairs": pairs,
        "crops": crop_log,
        "absent": absent,
    });
    std::fs::create_dir_all(report_dir)?;
    let tmp = report_dir.join("atlas-media.json.partial");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&report)?)?;
    std::fs::rename(&tmp, report_dir.join("atlas-media.json"))?;
    log(format!("atlas media: {} file names, done in {:.1} s; {}", rows.len(), t0.elapsed().as_secs_f64(), serde_json::to_string(&by_name)?));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, imageops};

    /// A deterministic textured picture (no disc data).
    fn picture(w: u32, h: u32, seed: u32) -> RgbImage {
        let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        };
        let blobs: Vec<(f32, f32, f32, [u8; 3])> = (0..40)
            .map(|_| ((rnd() % w) as f32, (rnd() % h) as f32, (8 + rnd() % 40) as f32, [(rnd() % 256) as u8, (rnd() % 256) as u8, (rnd() % 256) as u8]))
            .collect();
        RgbImage::from_fn(w, h, |x, y| {
            let mut p = [((x * 255) / w) as u8 / 2, ((y * 255) / h) as u8 / 2, 90];
            for (bx, by, r, c) in &blobs {
                if (x as f32 - bx).hypot(y as f32 - by) < *r {
                    p = *c;
                }
            }
            Rgb(p)
        })
    }

    fn pushpin(img: &RgbImage, x: u32, y: u32, side: u32) -> RgbImage {
        imageops::resize(&imageops::crop_imm(img, x, y, side, side).to_image(), 64, 64, imageops::FilterType::Triangle)
    }

    #[test]
    fn finds_a_crop_and_rejects_another_picture() {
        let img = picture(420, 300, 7);
        let q = Query::new(&pushpin(&img, 120, 60, 180));
        let m = best_crop(&Planes::new(&img), &q).unwrap();
        assert!(m.accepted(), "{m:?}");
        let (x, y, side) = m.bbox;
        assert!((side as i64 - 180).abs() <= 12 && (x as i64 - 120).abs() <= 12 && (y as i64 - 60).abs() <= 12, "{:?}", m.bbox);
        let other = best_crop(&Planes::new(&picture(420, 300, 99)), &q).unwrap();
        assert!(!other.accepted(), "{other:?}");
        // Two candidates: the right one wins; an equally good other picture would be ambiguous.
        let list = vec![("a", m.clone()), ("b", other)];
        assert_eq!(pick_crop(&list).unwrap().0, "a");
        let mut fake = m.clone();
        fake.crop = fake.crop.iter().map(|v| -v).collect();
        assert_eq!(pick_crop(&[("a", m), ("c", fake)]).unwrap_err(), "ambiguous");
    }

    #[test]
    fn thumbnail_feature_is_size_independent() {
        let img = picture(64, 64, 3);
        let big = imageops::resize(&img, 128, 128, imageops::FilterType::Triangle);
        assert!(rms(&thumb_feature(&img), &thumb_feature(&big)) < 4.0);
        assert!(rms(&thumb_feature(&img), &thumb_feature(&picture(64, 64, 4))) > 20.0);
    }

    #[test]
    fn grid_means_and_ncc() {
        let img = RgbImage::from_fn(4, 4, |x, _| Rgb([x as u8 * 10, 0, 0]));
        let p = Planes::new(&img);
        assert_eq!(p.rgb_grid(0, 0, 4, 4, 2)[..4], [5.0, 25.0, 5.0, 25.0]);
        assert!((ncc(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]) - 1.0).abs() < 1e-12);
        assert!((ncc(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn recording_keys() {
        let a = "\u{201C}Dance Tune\u{201D} performed by A and B, from Songs (Cat.# LDX 274 752) (p) Label. All rights reserved.";
        let b = "\"Dance Tune\" from Songs of the Pashai (Cat.# Le Chant du Monde LDX 274 752) (p)1990 Label.";
        assert_eq!(recording_key(a), Some(("dancetune".into(), "274752".into())));
        assert_eq!(recording_key(a), recording_key(b));
        assert_eq!(recording_key("Paul A. Souders/Corbis"), None);
        assert_eq!(recording_key("\"Song\" (no catalogue number)"), None);
        assert_eq!(photographer("Paul A. Souders/Corbis"), "paulasouders");
    }
}
