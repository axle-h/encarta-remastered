//! GeoGem geometry → MVT tiles in PMTiles.
//!
//! Each GeoFaceScale band is Encarta's own generalisation for one camera-altitude range. A tile
//! zoom `z` takes the band whose altitude range contains `geo::altitude_km(z)`, the highest
//! altitude shown with that tile zoom, so a tile is never denser than Encarta at any point of its
//! zoom range. Three archives:
//! - `vector/earth.pmtiles`: bands 0–7 (the altitudes of every MapStyle's normal range);
//! - `vector/earth-detail.pmtiles`: bands 8–9 (below 150 km: city insets, MinRect boxes, the
//!   VE styles), which only add to band 7 — the client overzooms `earth` underneath;
//! - `vector/moon.pmtiles`: the lunar classes (`MOON - …`), in lunar coordinates.
//!
//! Layers: `areas`, `lines`, `points` (points and radius points), `labels` (one anchor per named
//! area feature and band). Areas follow Encarta's even-odd fill: within one object, class and
//! cell, a ring inside an odd number of other rings is a hole.

use super::clip::{self, Rect};
use super::geo;
use super::mvt::{self, Geometry, Layer, Value};
use super::objects::{Classes, Objects};
use super::pmtiles::{Info, TileType, Writer};
use anyhow::Result;
use encarta_formats::mad::Atlas;
use encarta_formats::mad::cells::CellSystem;
use encarta_formats::mad::grid::Grid;
use encarta_formats::mad::schemas::GeoGemRecord;
use encarta_formats::mad::tables::GeometryKind;
use flate2::Compression;
use flate2::write::GzEncoder;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// World coordinates in fixed point: `[0, 1]` → `[0, 2^28]`.
const FIXED: f64 = (1u64 << 28) as f64;
/// Tile buffer in tile units (of [`mvt::EXTENT`]).
const BUFFER: f64 = 64.0;

type FP = (i32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    Points,
    Line,
    /// `parts[0]` is the exterior ring, the rest are its holes.
    Polygon,
    Label,
}

struct Piece {
    obj: u32,
    class: u16,
    shape: Shape,
    bbox: [i32; 4],
    parts: Vec<Vec<FP>>,
}

/// Which archive a class goes to and what of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tileset {
    Earth,
    EarthDetail,
    Moon,
}

impl Tileset {
    pub fn name(self) -> &'static str {
        match self {
            Tileset::Earth => "earth",
            Tileset::EarthDetail => "earth-detail",
            Tileset::Moon => "moon",
        }
    }
    pub fn path(self) -> String {
        format!("vector/{}.pmtiles", self.name())
    }
    fn bands(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Tileset::Earth | Tileset::Moon => 0..=7,
            Tileset::EarthDetail => 8..=9,
        }
    }
    /// The highest tile zoom written; MapLibre overzooms beyond.
    fn max_zoom_cap(self) -> u8 {
        12
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Include {
    geometry: bool,
    label_point: bool,
}

fn include(ts: Tileset, classes: &Classes, row: usize) -> Include {
    let (u, kind) = (classes.uses[row], classes.kinds[row]);
    if (ts == Tileset::Moon) != classes.is_moon(row) || !u.anything() {
        return Include::default();
    }
    let labelled_shape = u.label > 0 && kind != GeometryKind::Area;
    let geometry = match ts {
        Tileset::Earth | Tileset::Moon => u.needs_geometry() || labelled_shape,
        // Below 150 km only geometry drawn by default. Optional-only areas there (Island,
        // Physiographic region …) are whole-cell label squares: they become label points.
        Tileset::EarthDetail => u.geometry_default > 0 || labelled_shape,
    };
    Include { geometry, label_point: kind == GeometryKind::Area && u.label > 0 }
}

/// Band used by tile zoom `z`.
pub fn band_for_zoom(cells: &CellSystem, z: u8) -> usize {
    let alt_m = geo::altitude_km(z as f64) * 1000.0;
    cells.bands.iter().position(|b| alt_m >= b.min_scale as f64).unwrap_or(cells.bands.len() - 1)
}

/// `(zoom, band)` pairs of a tileset.
pub fn zoom_bands(cells: &CellSystem, ts: Tileset) -> Vec<(u8, usize)> {
    (0..=ts.max_zoom_cap()).map(|z| (z, band_for_zoom(cells, z))).filter(|(_, b)| ts.bands().contains(b)).collect()
}

fn to_fixed(lat: f64, lon: f64) -> FP {
    ((geo::lon_to_x(lon) * FIXED).round() as i32, (geo::lat_to_y(lat) * FIXED).round() as i32)
}

fn bbox_of(parts: &[Vec<FP>]) -> [i32; 4] {
    let mut b = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for p in parts.iter().flatten() {
        b[0] = b[0].min(p.0);
        b[1] = b[1].min(p.1);
        b[2] = b[2].max(p.0);
        b[3] = b[3].max(p.1);
    }
    b
}

/// Groups the area rings of one object/class/cell into polygons by even-odd nesting.
fn nest_rings(rings: Vec<Vec<FP>>) -> Vec<Vec<Vec<FP>>> {
    let n = rings.len();
    if n == 1 {
        return vec![rings];
    }
    let f: Vec<Vec<clip::P>> = rings.iter().map(|r| r.iter().map(|&(x, y)| (x as f64, y as f64)).collect()).collect();
    let boxes: Vec<[i32; 4]> = rings.iter().map(|r| bbox_of(std::slice::from_ref(r))).collect();
    let areas: Vec<f64> = f.iter().map(|r| clip::area2(r).abs()).collect();
    // Containers of each ring: rings that contain a probe point of it and are larger.
    let mut parent = vec![usize::MAX; n];
    let mut depth = vec![0usize; n];
    for i in 0..n {
        let probe = clip::interior_point(&f[i]);
        let mut best = usize::MAX;
        for j in 0..n {
            if i == j || areas[j] <= areas[i] {
                continue;
            }
            let (b, a) = (boxes[j], boxes[i]);
            if a[0] < b[0] || a[1] < b[1] || a[2] > b[2] || a[3] > b[3] {
                continue;
            }
            if clip::point_in_ring(probe, &f[j]) {
                depth[i] += 1;
                if best == usize::MAX || areas[j] < areas[best] {
                    best = j;
                }
            }
        }
        parent[i] = best;
    }
    let mut polys: Vec<Vec<Vec<FP>>> = Vec::new();
    let mut slot = vec![usize::MAX; n];
    let mut rings: Vec<Option<Vec<FP>>> = rings.into_iter().map(Some).collect();
    for i in 0..n {
        if depth[i].is_multiple_of(2) {
            slot[i] = polys.len();
            polys.push(vec![rings[i].take().unwrap()]);
        }
    }
    for i in 0..n {
        if depth[i] % 2 == 1 {
            let p = parent[i];
            let r = rings[i].take().unwrap();
            if p != usize::MAX && slot[p] != usize::MAX {
                polys[slot[p]].push(r);
            } else {
                polys.push(vec![r]);
            }
        }
    }
    polys
}

struct Decoder<'a> {
    gem: Grid<'a>,
    cells: CellSystem,
    classes: &'a Classes,
    objects: &'a Objects,
}

impl Decoder<'_> {
    /// Every piece of band `band` that `ts` includes, placed per key (shared records repeat).
    fn band_pieces(&self, ts: Tileset, band: usize, region: Option<(f64, f64, f64, f64)>) -> Result<Vec<Piece>> {
        let b = self.cells.bands[band];
        let (lo, hi) = (b.first_key, b.first_key + b.key_count());
        let in_region = |key: u32| match (region, self.cells.cell(key)) {
            (Some((w, s, e, n)), Some(c)) => {
                let (cw, cs, ce, cn) = c.bounds();
                cw <= e && ce >= w && cs <= n && cn >= s
            }
            _ => true,
        };
        let runs: Vec<_> = self.gem.index.runs().iter().filter(|r| r.first < hi && r.last() >= lo).cloned().collect();
        let incl: Vec<Include> = (0..self.classes.classes.len()).map(|c| include(ts, self.classes, c)).collect();
        let pieces: Vec<Vec<Piece>> = runs
            .par_iter()
            .map(|run| -> Result<Vec<Piece>> {
                let bytes = self.gem.bytes(run.offset, run.length)?;
                let rec = GeoGemRecord::decode(&mut self.gem.decoder(bytes), &self.classes.kinds)?;
                let mut out = Vec::new();
                for key in run.first.max(lo)..=run.last().min(hi - 1) {
                    let Some(cell) = self.cells.cell(key) else { continue };
                    if !in_region(key) {
                        continue;
                    }
                    for o in &rec.objects {
                        let Some(obj) = self.objects.index(o.object) else { continue };
                        // Area rings per class, kept in record order.
                        let mut areas: Vec<(u16, Vec<Vec<FP>>)> = Vec::new();
                        for p in &o.parts {
                            let c = p.class as usize;
                            let inc = incl[c];
                            if !inc.geometry && !inc.label_point {
                                continue;
                            }
                            let pts: Vec<FP> = rec
                                .points(p)
                                .map(|(x, y)| {
                                    let (lat, lon) = cell.lat_lon(x as f64, y as f64);
                                    to_fixed(lat, lon)
                                })
                                .collect();
                            match self.classes.kinds[c] {
                                GeometryKind::Area => {
                                    let mut ring = pts;
                                    if ring.len() > 1 && ring.first() == ring.last() {
                                        ring.pop();
                                    }
                                    if ring.len() < 3 {
                                        continue;
                                    }
                                    match areas.iter_mut().find(|a| a.0 == p.class) {
                                        Some(a) => a.1.push(ring),
                                        None => areas.push((p.class, vec![ring])),
                                    }
                                }
                                GeometryKind::Line if inc.geometry && pts.len() >= 2 => {
                                    out.push(Piece { obj, class: p.class, shape: Shape::Line, bbox: bbox_of(std::slice::from_ref(&pts)), parts: vec![pts] });
                                }
                                GeometryKind::Point | GeometryKind::RadiusPoint if inc.geometry && !pts.is_empty() => {
                                    out.push(Piece { obj, class: p.class, shape: Shape::Points, bbox: bbox_of(std::slice::from_ref(&pts)), parts: vec![pts] });
                                }
                                _ => {}
                            }
                        }
                        for (class, rings) in areas {
                            for poly in nest_rings(rings) {
                                out.push(Piece { obj, class, shape: Shape::Polygon, bbox: bbox_of(&poly[..1]), parts: poly });
                            }
                        }
                    }
                }
                Ok(out)
            })
            .collect::<Result<_>>()?;
        let mut pieces: Vec<Piece> = pieces.into_iter().flatten().collect();
        // Label anchors: one per (object, class) of labelled area classes.
        let mut best: HashMap<(u32, u16), (f64, usize, [i32; 4])> = HashMap::new();
        for (i, p) in pieces.iter().enumerate() {
            if p.shape != Shape::Polygon || !incl[p.class as usize].label_point {
                continue;
            }
            let ring: Vec<clip::P> = p.parts[0].iter().map(|&(x, y)| (x as f64, y as f64)).collect();
            let a = clip::area2(&ring).abs();
            let e = best.entry((p.obj, p.class)).or_insert((-1.0, i, p.bbox));
            e.2 = [e.2[0].min(p.bbox[0]), e.2[1].min(p.bbox[1]), e.2[2].max(p.bbox[2]), e.2[3].max(p.bbox[3])];
            if a > e.0 {
                e.0 = a;
                e.1 = i;
            }
        }
        let mut labels: Vec<Piece> = best
            .into_iter()
            .map(|((obj, class), (_, i, bbox))| {
                let o = &self.objects.objs[obj as usize];
                let goto = o.goto_point(&self.cells).map(|(lat, lon)| to_fixed(lat, lon)).filter(|g| {
                    let pad = ((bbox[2] - bbox[0]).max(bbox[3] - bbox[1]) / 20).max(1);
                    g.0 >= bbox[0] - pad && g.0 <= bbox[2] + pad && g.1 >= bbox[1] - pad && g.1 <= bbox[3] + pad
                });
                let at = goto.unwrap_or_else(|| {
                    let ring: Vec<clip::P> = pieces[i].parts[0].iter().map(|&(x, y)| (x as f64, y as f64)).collect();
                    let p = clip::interior_point(&ring);
                    (p.0.round() as i32, p.1.round() as i32)
                });
                Piece { obj, class, shape: Shape::Label, bbox: [at.0, at.1, at.0, at.1], parts: vec![vec![at]] }
            })
            .collect();
        labels.sort_by_key(|l| (l.obj, l.class));
        pieces.retain(|p| p.shape != Shape::Polygon || incl[p.class as usize].geometry);
        pieces.extend(labels);
        Ok(pieces)
    }
}

fn props(objects: &Objects, classes: &Classes, obj: u32, class: u16, label: bool) -> Vec<(&'static str, Value)> {
    let o = &objects.objs[obj as usize];
    let c = &classes.classes[class as usize];
    let mut v = vec![("class", Value::Uint(class as u64)), ("class_name", Value::Str(c.name.clone())), ("entity", Value::Uint(o.entity as u64))];
    if !o.name.primary.is_empty() {
        v.push(("name", Value::Str(o.name.primary.clone())));
    }
    if let Some(a) = &o.name.abbr {
        v.push(("abbr", Value::Str(a.clone())));
    }
    if !o.name.alternatives.is_empty() {
        v.push(("alt_names", Value::Str(o.name.alternatives.join("; "))));
    }
    if let Some(n) = &o.name_alt_lcid {
        v.push(("name_0009", Value::Str(n.split('\u{4}').next().unwrap_or(n).split('\u{2}').next().unwrap_or(n).to_string())));
    }
    if o.name.anonymous {
        v.push(("anonymous", Value::Bool(true)));
    }
    if label || matches!(classes.kinds[class as usize], GeometryKind::Point | GeometryKind::RadiusPoint) {
        v.push(("priority", Value::Uint(c.priority as u64)));
    }
    if label && o.goto_km > 0 {
        v.push(("goto_km", Value::Uint(o.goto_km as u64)));
    }
    v
}

/// Tile units of a fixed-point world coordinate for tile `(z, x, y)`, with a world shift.
fn to_tile(p: FP, z: u8, tx: u32, ty: u32, shift: i32) -> clip::P {
    let s = mvt::EXTENT as f64 * (1u64 << z) as f64 / FIXED;
    ((p.0 as f64 + shift as f64 * FIXED) * s - tx as f64 * mvt::EXTENT as f64, p.1 as f64 * s - ty as f64 * mvt::EXTENT as f64)
}

fn quantise(pts: &[clip::P]) -> Vec<FP> {
    let mut out: Vec<FP> = Vec::with_capacity(pts.len());
    for p in pts {
        let q = (p.0.round() as i32, p.1.round() as i32);
        if out.last() != Some(&q) {
            out.push(q);
        }
    }
    out
}

#[derive(Default)]
struct TileStats {
    features: [u64; 4],
}

#[allow(clippy::too_many_arguments)]
fn encode_tile(pieces: &[Piece], refs: &[(u32, i8)], z: u8, tx: u32, ty: u32, objects: &Objects, classes: &Classes, st: &mut TileStats) -> Option<Vec<u8>> {
    let e = mvt::EXTENT as f64;
    let rect = Rect { x0: -BUFFER, y0: -BUFFER, x1: e + BUFFER, y1: e + BUFFER };
    // Group by (object, class, shape) in first-appearance order.
    let mut groups: Vec<((u32, u16, Shape), Geometry, bool)> = Vec::new();
    let mut index: HashMap<(u32, u16, Shape), usize> = HashMap::new();
    for &(pi, shift) in refs {
        let p = &pieces[pi as usize];
        let k = (p.obj, p.class, p.shape);
        let gi = *index.entry(k).or_insert_with(|| {
            groups.push((k, Geometry::default(), false));
            groups.len() - 1
        });
        let g = &mut groups[gi];
        let tp = |pts: &[FP]| -> Vec<clip::P> { pts.iter().map(|&q| to_tile(q, z, tx, ty, shift as i32)).collect() };
        match p.shape {
            Shape::Points | Shape::Label => {
                let inside: Vec<FP> = quantise(&tp(&p.parts[0])).into_iter().filter(|q| q.0 >= 0 && q.1 >= 0 && (q.0 as f64) < e && (q.1 as f64) < e).collect();
                if !inside.is_empty() {
                    g.1.points(&inside);
                    g.2 = true;
                }
            }
            Shape::Line => {
                for piece in clip::clip_line(&tp(&p.parts[0]), &rect) {
                    let q = quantise(&piece);
                    if q.len() >= 2 {
                        g.1.line(&q);
                        g.2 = true;
                    }
                }
            }
            Shape::Polygon => {
                let mut first = true;
                for (ri, ring) in p.parts.iter().enumerate() {
                    let mut q = quantise(&clip::clip_ring(&tp(ring), &rect));
                    if q.len() > 1 && q.first() == q.last() {
                        q.pop();
                    }
                    let a = mvt::signed_area2(&q);
                    if q.len() < 3 || a == 0 {
                        if ri == 0 {
                            break;
                        }
                        continue;
                    }
                    let exterior = ri == 0;
                    if (a > 0) != exterior {
                        q.reverse();
                    }
                    if first && !exterior {
                        break;
                    }
                    first = false;
                    g.1.ring(&q);
                    g.2 = true;
                }
            }
        }
    }
    let mut layers = [Layer::new("areas"), Layer::new("lines"), Layer::new("points"), Layer::new("labels")];
    for ((obj, class, shape), geom, any) in groups {
        if !any {
            continue;
        }
        let (li, gt) = match shape {
            Shape::Polygon => (0, mvt::POLYGON),
            Shape::Line => (1, mvt::LINESTRING),
            Shape::Points => (2, mvt::POINT),
            Shape::Label => (3, mvt::POINT),
        };
        let entity = objects.objs[obj as usize].entity as u64;
        layers[li].add(Some(entity), gt, &geom, &props(objects, classes, obj, class, shape == Shape::Label));
        st.features[li] += 1;
    }
    if layers.iter().all(|l| l.len() == 0) {
        return None;
    }
    let raw = mvt::encode_tile(&layers);
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&raw).ok()?;
    gz.finish().ok()
}

#[derive(Debug, Clone, Serialize)]
pub struct ZoomReport {
    pub zoom: u8,
    pub band: usize,
    pub altitude_km_at_zoom: f64,
    pub pieces: u64,
    pub tiles: u64,
    pub bytes: u64,
    pub features: BTreeMap<&'static str, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TilesetReport {
    pub path: String,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub zooms: Vec<ZoomReport>,
    pub tiles: u64,
    pub distinct_tiles: u64,
    pub bytes: u64,
    pub classes_with_geometry: Vec<u16>,
    pub classes_as_label_points: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VectorReport {
    pub tilesets: Vec<TilesetReport>,
    /// PDCAttribute rows that no rule of any MapStyle draws, labels or hit-tests (geometry only
    /// feeds the gazetteer extents).
    pub classes_never_used: Vec<(u16, String)>,
    pub zoom_bands: Vec<(u8, usize, f64)>,
    pub seconds: f64,
}

fn layer_meta(classes: &Classes) -> Json {
    let fields = json!({ "class": "Number", "class_name": "String", "entity": "Number", "name": "String", "abbr": "String",
                          "alt_names": "String", "name_0009": "String", "anonymous": "Boolean", "priority": "Number", "goto_km": "Number" });
    let _ = classes;
    json!(["areas", "lines", "points", "labels"].iter().map(|l| json!({ "id": l, "fields": fields })).collect::<Vec<_>>())
}

pub fn run(atlas: &Atlas, objects: &Objects, classes: &Classes, out: &Path, log: &dyn Fn(String)) -> Result<VectorReport> {
    let t0 = std::time::Instant::now();
    let cells = atlas.cells()?;
    let dec = Decoder { gem: atlas.geo_gem()?, cells: cells.clone(), classes, objects };
    let mut tilesets = Vec::new();
    for ts in [Tileset::Earth, Tileset::EarthDetail, Tileset::Moon] {
        let mut zb = zoom_bands(&cells, ts);
        if ts == Tileset::Moon {
            // Moon data stops at band 2; overzoom beyond.
            zb.retain(|&(_, b)| b <= 2);
        }
        let Some(&(min_zoom, _)) = zb.first() else { continue };
        let max_zoom = zb.last().unwrap().0;
        let path = out.join(ts.path());
        let writer = Mutex::new(Writer::create(&path)?);
        let mut zooms = Vec::new();
        let mut done_band = usize::MAX;
        let mut pieces = Vec::new();
        for &(z, band) in &zb {
            if band != done_band {
                pieces = dec.band_pieces(ts, band, None)?;
                done_band = band;
            }
            // Bucket pieces by tile (sorted list instead of a map).
            let n = 1i64 << z;
            let unit = FIXED / n as f64;
            let buf = BUFFER / mvt::EXTENT as f64 * unit;
            let mut refs: Vec<(u64, u32, i8)> = pieces
                .par_iter()
                .enumerate()
                .flat_map_iter(|(i, p)| {
                    let mut v = Vec::new();
                    let tr = |a: f64| (a / unit).floor() as i64;
                    let (y0, y1) = (tr(p.bbox[1] as f64 - buf).max(0), tr(p.bbox[3] as f64 + buf).min(n - 1));
                    for shift in [-1i64, 0, 1] {
                        let off = shift as f64 * FIXED;
                        let (x0, x1) = (tr(p.bbox[0] as f64 + off - buf).max(0), tr(p.bbox[2] as f64 + off + buf).min(n - 1));
                        for x in x0..=x1 {
                            for y in y0..=y1 {
                                v.push((((x as u64) << 32) | y as u64, i as u32, shift as i8));
                            }
                        }
                    }
                    v.into_iter()
                })
                .collect();
            refs.par_sort_unstable();
            let mut groups: Vec<(usize, usize)> = Vec::new();
            let mut s = 0;
            for i in 1..=refs.len() {
                if i == refs.len() || refs[i].0 != refs[s].0 {
                    groups.push((s, i));
                    s = i;
                }
            }
            let stats = Mutex::new((0u64, 0u64, [0u64; 4]));
            groups.par_iter().try_for_each(|&(a, b)| -> Result<()> {
                let key = refs[a].0;
                let (tx, ty) = ((key >> 32) as u32, key as u32);
                let mut list: Vec<(u32, i8)> = refs[a..b].iter().map(|r| (r.1, r.2)).collect();
                list.sort_unstable();
                let mut st = TileStats::default();
                if let Some(t) = encode_tile(&pieces, &list, z, tx, ty, objects, classes, &mut st) {
                    let len = t.len() as u64;
                    writer.lock().unwrap().add(z, tx, ty, &t)?;
                    let mut s = stats.lock().unwrap();
                    s.0 += 1;
                    s.1 += len;
                    for k in 0..4 {
                        s.2[k] += st.features[k];
                    }
                }
                Ok(())
            })?;
            let (tiles, bytes, f) = stats.into_inner().unwrap();
            let features = ["areas", "lines", "points", "labels"].iter().zip(f).map(|(k, v)| (*k, v)).collect();
            log(format!("  {} z{z} (band {band}): {} pieces → {tiles} tiles, {:.1} MB", ts.name(), pieces.len(), bytes as f64 / 1e6));
            zooms.push(ZoomReport { zoom: z, band, altitude_km_at_zoom: geo::altitude_km(z as f64), pieces: pieces.len() as u64, tiles, bytes, features });
        }
        drop(pieces);
        let (w, s, e, n) = (-180.0, -geo::MAX_LAT, 180.0, geo::MAX_LAT);
        let info = Info { tile_type: TileType::Mvt, gzip_tiles: true, min_zoom, max_zoom, bounds: (w, s, e, n), center: (0.0, 20.0, min_zoom) };
        let meta = json!({
            "name": format!("Encarta {}", ts.name()), "format": "pbf", "type": "overlay",
            "attribution": "Microsoft Encarta Premium 2009 (converted from the user's disc)",
            "minzoom": min_zoom, "maxzoom": max_zoom, "vector_layers": layer_meta(classes),
            "encarta": { "zoom_bands": zb, "view_p_over_m": geo::VIEW_P_OVER_M },
        });
        let stats = writer.into_inner().unwrap().finish(&info, &meta)?;
        let incl: Vec<Include> = (0..classes.classes.len()).map(|c| include(ts, classes, c)).collect();
        tilesets.push(TilesetReport {
            path: ts.path(),
            min_zoom,
            max_zoom,
            tiles: stats.addressed,
            distinct_tiles: stats.contents,
            bytes: stats.bytes,
            zooms,
            classes_with_geometry: (0..incl.len()).filter(|&c| incl[c].geometry).map(|c| c as u16).collect(),
            classes_as_label_points: (0..incl.len()).filter(|&c| incl[c].label_point).map(|c| c as u16).collect(),
        });
        log(format!("  {}: {} tiles, {:.1} MB", ts.path(), stats.addressed, stats.bytes as f64 / 1e6));
    }
    let classes_never_used = (0..classes.classes.len()).filter(|&c| !classes.uses[c].anything()).map(|c| (c as u16, classes.classes[c].name.clone())).collect();
    let zoom_bands = (0..=12u8).map(|z| (z, band_for_zoom(&cells, z), geo::altitude_km(z as f64))).collect();
    Ok(VectorReport { tilesets, classes_never_used, zoom_bands, seconds: t0.elapsed().as_secs_f64() })
}

/// Classes that the earth-detail archive carries (geometry or label points) and that have data
/// in bands 8–9.
pub fn detail_classes(atlas: &Atlas, classes: &Classes) -> Result<std::collections::BTreeSet<u16>> {
    let gem = atlas.geo_gem()?;
    let cells = atlas.cells()?;
    let lo = cells.bands[8.min(cells.bands.len() - 1)].first_key;
    let runs: Vec<_> = gem.index.runs().iter().filter(|r| r.last() >= lo).cloned().collect();
    let sets: Vec<std::collections::BTreeSet<u16>> = runs
        .par_iter()
        .map(|r| -> Result<_> {
            let rec = GeoGemRecord::decode(&mut gem.decoder(gem.bytes(r.offset, r.length)?), &classes.kinds)?;
            Ok(rec.objects.iter().flat_map(|o| o.parts.iter().map(|p| p.class)).collect())
        })
        .collect::<Result<_>>()?;
    Ok(sets
        .into_iter()
        .flatten()
        .filter(|&c| {
            let i = include(Tileset::EarthDetail, classes, c as usize);
            i.geometry || i.label_point
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small-region run on the user's disc (skipped without `extracted/`): Denmark, band 7, z8.
    #[test]
    fn denmark_from_disc() {
        let root = std::env::var_os("ENCARTA_EXTRACTED")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"));
        let Ok(atlas) = Atlas::open_edition(&root, encarta_formats::mad::Edition::World) else {
            eprintln!("skipped: no extracted disc at {}", root.display());
            return;
        };
        let classes = Classes::load(&atlas).unwrap();
        let objects = Objects::load(&atlas).unwrap();
        let cells = atlas.cells().unwrap();
        assert_eq!(band_for_zoom(&cells, 8), 7);
        assert_eq!(band_for_zoom(&cells, 9), 8);
        let dec = Decoder { gem: atlas.geo_gem().unwrap(), cells: cells.clone(), classes: &classes, objects: &objects };
        let pieces = dec.band_pieces(Tileset::Earth, 7, Some((11.0, 55.0, 13.5, 56.5))).unwrap();
        assert!(pieces.len() > 1000);
        // Tile z8 containing Copenhagen (12.57 E, 55.68 N).
        let (z, n) = (8u8, 256.0);
        let (tx, ty) = ((geo::lon_to_x(12.57) * n) as u32, (geo::lat_to_y(55.68) * n) as u32);
        let unit = FIXED / n;
        let refs: Vec<(u32, i8)> = pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| (p.bbox[0] as f64) < (tx + 1) as f64 * unit && (p.bbox[2] as f64) > tx as f64 * unit && (p.bbox[1] as f64) < (ty + 1) as f64 * unit && (p.bbox[3] as f64) > ty as f64 * unit)
            .map(|(i, _)| (i as u32, 0))
            .collect();
        let mut st = TileStats::default();
        let gz = encode_tile(&pieces, &refs, z, tx, ty, &objects, &classes, &mut st).unwrap();
        let mut raw = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&gz[..]), &mut raw).unwrap();
        let layers = mvt::decode_tile(&raw).unwrap();
        let names: Vec<String> = layers
            .iter()
            .flat_map(|l| l.features.iter())
            .filter_map(|f| f.props.iter().find(|p| p.0 == "name").and_then(|p| if let Value::Str(s) = &p.1 { Some(s.clone()) } else { None }))
            .collect();
        assert!(names.iter().any(|n| n == "Copenhagen" || n == "København"), "{:?}", &names[..names.len().min(40)]);
        let areas = layers.iter().find(|l| l.name == "areas").unwrap();
        for f in &areas.features {
            assert!(mvt::signed_area2(&f.parts[0]) > 0, "first ring of a polygon feature is exterior");
        }
    }

    #[test]
    fn even_odd_nesting() {
        let sq = |x0: i32, y0: i32, s: i32| vec![(x0, y0), (x0 + s, y0), (x0 + s, y0 + s), (x0, y0 + s)];
        // Outer, a hole in it, an island in the hole, and a separate ring.
        let polys = nest_rings(vec![sq(0, 0, 100), sq(10, 10, 80), sq(20, 20, 10), sq(200, 0, 10)]);
        assert_eq!(polys.len(), 3);
        assert_eq!(polys[0].len(), 2, "outer with its hole");
        assert_eq!(polys[1], vec![sq(20, 20, 10)]);
        assert_eq!(polys[2], vec![sq(200, 0, 10)]);
    }

    #[test]
    fn tile_encoding_orients_and_clips() {
        // One polygon with a hole crossing the tile edge; one line; one point outside the tile.
        let objects = Objects::synthetic(&["Lake", "River", "Town"]);
        let classes = Classes::synthetic(&[GeometryKind::Area, GeometryKind::Line, GeometryKind::Point]);
        let unit = FIXED / 2.0; // zoom 1
        let f = |x: f64, y: f64| ((x * unit) as i32, (y * unit) as i32);
        let outer = vec![f(0.5, 0.5), f(0.5, 1.5), f(1.5, 1.5), f(1.5, 0.5)]; // crosses into tiles (1,1),(0,0)…
        let hole = vec![f(0.8, 0.8), f(0.9, 0.8), f(0.9, 0.9), f(0.8, 0.9)];
        let pieces = vec![
            Piece { obj: 0, class: 0, shape: Shape::Polygon, bbox: bbox_of(std::slice::from_ref(&outer)), parts: vec![outer, hole] },
            Piece { obj: 1, class: 1, shape: Shape::Line, bbox: [0; 4], parts: vec![vec![f(0.1, 0.1), f(0.9, 0.9), f(1.9, 1.9)]] },
            Piece { obj: 2, class: 2, shape: Shape::Points, bbox: [0; 4], parts: vec![vec![f(1.5, 0.5)]] },
        ];
        let mut st = TileStats::default();
        let gz = encode_tile(&pieces, &[(0, 0), (1, 0), (2, 0)], 1, 0, 0, &objects, &classes, &mut st).unwrap();
        let mut raw = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&gz[..]), &mut raw).unwrap();
        let layers = mvt::decode_tile(&raw).unwrap();
        let areas = layers.iter().find(|l| l.name == "areas").unwrap();
        let rings = &areas.features[0].parts;
        assert_eq!(rings.len(), 2);
        assert!(mvt::signed_area2(&rings[0]) > 0 && mvt::signed_area2(&rings[1]) < 0);
        assert!(rings[0].iter().all(|p| p.0 <= 4096 + 64 && p.1 <= 4096 + 64));
        let lines = layers.iter().find(|l| l.name == "lines").unwrap();
        assert!(lines.features[0].parts[0].last().unwrap().0 <= 4096 + 64);
        assert!(!layers.iter().any(|l| l.name == "points"), "point outside the tile is dropped");
        assert_eq!(areas.features[0].id, Some(1000));
    }
}
