//! Pixel classes and the pens' "draw over" masks (mad-styling.md §5.2), approximated for MapLibre.
//!
//! MOBB gives every pixel a class (from the raster CMT `kind`, or from the rule that drew it) and
//! a pen sets a pixel only if its `over` mask has the bit of that class. MapLibre cannot test the
//! pixel underneath, so the converter decides it per line part instead: the vector stage splits
//! the lines of masked classes into runs over water and over land (`water` property, from the
//! HYPSO raster), and the styles filter each pen layer by the classes its mask allows over water
//! and over land for the style's own raster ([`SurfaceClasses`]).
//!
//! Simplification (documented in docs/atlas-output.md): the class under a line is taken from the
//! raster only, not from the area fills drawn before it. For HYPSO this gives the same classes,
//! because water areas use `b14 = 1` (class 2, like raster water) and land fills `b14 = 0` (class 4,
//! like raster land).

use super::geo;
use super::raster::TileCache;
use anyhow::Result;
use encarta_formats::mad::octahedral::{from_lat_lon, pixel_of, vector_lat_lon};
use encarta_formats::mad::rmap::{self, Look, Raster};
use i_overlay::core::extract::BooleanExtractionBuffer;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::point::IntPoint;
use std::collections::HashMap;

/// Every mask bit (1, 2, 4, 8, 16): the pen draws over everything.
pub const ALL_BITS: u8 = 31;

/// Pixel class of a raster value with CMT `kind` (MOBB: 0 → 1, 1 → 2, 2 → 4, 3 → 5, other → 0).
pub fn pixel_class(kind: u8) -> u8 {
    match kind {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 5,
        _ => 0,
    }
}

/// Mask bit of a pixel class (the 8 bytes `10 02 02 02 01 00 08 10`; classes ≥ 8 → 0).
pub fn class_bit(class: u8) -> u8 {
    const BITS: [u8; 8] = [16, 2, 2, 2, 1, 0, 8, 16];
    BITS.get(class as usize).copied().unwrap_or(0)
}

/// GeoPenDef `over[0…4]` → mask bits 1, 2, 4, 8, 16.
pub fn pen_mask(over: &[u8; 5]) -> u8 {
    (0..5).filter(|&k| over[k] != 0).map(|k| 1u8 << k).sum()
}

/// May a pen with `mask` draw over a pixel of `class`?
pub fn allows(mask: u8, class: u8) -> bool {
    mask & class_bit(class) != 0
}

/// HYPSO values are water when their CMT `kind` is 0 (mad.md §5.4, correction).
pub fn water_values(look: &Look) -> [bool; 256] {
    std::array::from_fn(|v| look.values[v].kind == 0)
}

/// Rec. 601 luma of a colour (0…255). The spec says only "luminance" (mad-styling.md §6).
pub fn luminance(c: [u8; 3]) -> f64 {
    0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64
}

/// Point lookups of raster values (nearest pixel of one octahedral level).
pub struct Sampler<'r, 'a> {
    cache: TileCache<'r, 'a>,
    level: u32,
    side: u32,
    /// Source tiles `(x, y)` of `level` whose pixels are all water or all land (`None`: mixed).
    uniform: HashMap<(u32, u32), Option<bool>>,
}

impl<'r, 'a> Sampler<'r, 'a> {
    pub fn new(raster: &'r Raster<'a>, level: u32) -> Self {
        let level = level.min(raster.info().max_level);
        Sampler { cache: TileCache::new(raster, 64), level, side: rmap::image_side(level) as u32, uniform: HashMap::new() }
    }

    pub fn value(&mut self, lat: f64, lon: f64) -> Result<u8> {
        let (row, col) = pixel_of(lat, lon, self.side);
        self.cache.pixel(self.level, row as usize, col as usize)
    }

    /// Size of one pixel in degrees (approximately; the octahedral pixels vary a little).
    pub fn pixel_degrees(&self) -> f64 {
        180.0 / self.side as f64
    }

    /// Pixels across the world of the Web Mercator grid that resolves this level at the equator
    /// (the octahedral pixel size rounded down to a power of two).
    pub fn mercator_grid(&self) -> u64 {
        (2 * self.side as u64).next_power_of_two()
    }

    /// Water flags (`water`: the values that are) of a block of points: rows at latitudes
    /// `lat(0…h-1)`, columns at longitudes `lon0 + (k + 0.5)·dlon`, written to `out[r·stride + k]`.
    /// Within one octahedral face the corners are projected exactly and the points in between
    /// bilinearly (the projection is smooth there); a block whose corners fall in one source tile
    /// whose pixels are all water or all land is filled without lookups.
    #[allow(clippy::too_many_arguments)]
    pub fn water_block(&mut self, lat: impl Fn(usize) -> f64, lon0: f64, dlon: f64, w: usize, h: usize, water: &[bool; 256], out: &mut [bool], stride: usize) -> Result<()> {
        let half = self.side as f64 / 2.0;
        let lon = |k: usize| lon0 + (k as f64 + 0.5) * dlon;
        let (n, s, west, east) = (lat(0), lat(h - 1), lon(0), lon(w - 1));
        let quadrant = |l: f64| ((l + 180.0).rem_euclid(360.0) / 90.0).floor() as i32;
        if (n >= 0.0) != (s >= 0.0) || quadrant(west) != quadrant(east) {
            // Across a face edge: every point exactly.
            for r in 0..h {
                for k in 0..w {
                    out[r * stride + k] = water[self.value(lat(r), lon(k))? as usize];
                }
            }
            return Ok(());
        }
        let corners = [from_lat_lon(n, west, half), from_lat_lon(n, east, half), from_lat_lon(s, west, half), from_lat_lon(s, east, half)];
        let px = |(u, v): (f64, f64)| ((v + half).floor().clamp(0.0, self.side as f64 - 1.0) as usize, (u + half).floor().clamp(0.0, self.side as f64 - 1.0) as usize);
        let tile = |(r, c): (usize, usize)| (r / rmap::TILE, c / rmap::TILE);
        let t0 = tile(px(corners[0]));
        if corners.iter().all(|&c| tile(px(c)) == t0) {
            let (x, y) = (t0.0 as u32, t0.1 as u32);
            let u = match self.uniform.get(&(x, y)) {
                Some(&u) => u,
                None => {
                    let u = self.cache.uniform_tile(self.level, x, y, water)?;
                    self.uniform.insert((x, y), u);
                    u
                }
            };
            if let Some(v) = u {
                for r in 0..h {
                    out[r * stride..r * stride + w].fill(v);
                }
                return Ok(());
            }
        }
        let lerp = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for r in 0..h {
            let t = if h > 1 { r as f64 / (h - 1) as f64 } else { 0.0 };
            let (a, b) = (lerp(corners[0], corners[2], t), lerp(corners[1], corners[3], t));
            for k in 0..w {
                let (row, col) = px(lerp(a, b, if w > 1 { k as f64 / (w - 1) as f64 } else { 0.0 }));
                out[r * stride + k] = water[self.cache.pixel(self.level, row, col)? as usize];
            }
        }
        Ok(())
    }
}

/// The pixel classes a style's raster puts under water and under land, and the mean brightness
/// of its land (for tint ramps, mad-styling.md §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceClasses {
    pub water: u8,
    pub land: u8,
    pub land_luminance: f64,
}

impl SurfaceClasses {
    /// Which of (over water, over land) a pen with `mask` may draw on.
    pub fn pen_sides(&self, mask: u8) -> (bool, bool) {
        (allows(mask, self.water), allows(mask, self.land))
    }
}

/// `n` points spread evenly over the sphere (Fibonacci lattice), as (lat, lon).
pub fn sphere_points(n: usize) -> Vec<(f64, f64)> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - z * z).sqrt();
            let t = golden * i as f64;
            vector_lat_lon([r * t.cos(), r * t.sin(), z])
        })
        .collect()
}

/// Classes and land brightness of `look` of `raster`, measured at sample points that the HYPSO
/// raster classifies as water or land. Majority class on each side.
pub fn calibrate(hypso: &Raster, raster: &Raster, look: &Look, samples: usize) -> Result<SurfaceClasses> {
    let hypso_look = hypso.look("HYPSO").or_else(|| hypso.looks.first()).ok_or_else(|| anyhow::anyhow!("HYPSO has no LOOK"))?;
    let water = water_values(hypso_look);
    let mut h = Sampler::new(hypso, 4);
    let mut r = Sampler::new(raster, 4);
    let (mut wc, mut lc) = ([0u32; 256], [0u32; 256]);
    let (mut lum, mut n_land) = (0.0, 0u32);
    for (lat, lon) in sphere_points(samples) {
        let is_water = water[h.value(lat, lon)? as usize];
        let v = look.values[r.value(lat, lon)? as usize];
        let class = pixel_class(v.kind) as usize;
        if is_water {
            wc[class] += 1;
        } else {
            lc[class] += 1;
            lum += luminance(v.rgb);
            n_land += 1;
        }
    }
    let major = |c: &[u32; 256]| (0..256).max_by_key(|&i| c[i]).unwrap_or(0) as u8;
    Ok(SurfaceClasses { water: major(&wc), land: major(&lc), land_luminance: if n_land > 0 { lum / n_land as f64 } else { 128.0 } })
}

/// A run of a line: over water?, and its (lat, lon) points.
pub type Run = (bool, Vec<(f64, f64)>);

/// Splits a line (lat, lon points) into runs over water and over land. Segments are sampled
/// every `step` degrees; where the class changes the crossing is found by bisection and the
/// crossing point ends one run and starts the next.
pub fn split_line<F: FnMut(f64, f64) -> Result<bool>>(pts: &[(f64, f64)], step: f64, mut is_water: F) -> Result<Vec<Run>> {
    let mut runs: Vec<Run> = Vec::new();
    let Some(&first) = pts.first() else { return Ok(runs) };
    let mut cur = is_water(first.0, first.1)?;
    let mut run = vec![first];
    let lerp = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b.0 - a.0).abs().max((b.1 - a.1).abs());
        let n = ((len / step.max(1e-9)).ceil() as usize).clamp(1, 256);
        let mut t0 = 0.0;
        for k in 1..=n {
            let t1 = k as f64 / n as f64;
            let p = lerp(a, b, t1);
            let wv = is_water(p.0, p.1)?;
            if wv != cur {
                // Bisect between t0 (class `cur`) and t1 (class `wv`).
                let (mut lo, mut hi) = (t0, t1);
                for _ in 0..10 {
                    let mid = (lo + hi) / 2.0;
                    let m = lerp(a, b, mid);
                    if is_water(m.0, m.1)? == cur {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let x = lerp(a, b, (lo + hi) / 2.0);
                run.push(x);
                runs.push((cur, std::mem::replace(&mut run, vec![x])));
                cur = wv;
            }
            t0 = t1;
        }
        run.push(b);
    }
    runs.push((cur, run));
    runs.retain(|(_, r)| r.len() >= 2);
    Ok(runs)
}

/// Fixed-point world coordinates of the vector stage (`[0, 1]` → `[0, 2^28]`, y south).
pub type FP = (i32, i32);
/// A polygon: exterior ring first, then its holes (rings not closed).
pub type Rings = Vec<Vec<FP>>;

/// The water mask over a window of a Web Mercator pixel grid, for splitting area fills.
/// Pixel `(i, j)` covers fixed x `[i·px, (i+1)·px)` and y `[j·px, (j+1)·px)`. Rows and columns
/// beyond the world repeat the edge pixels, so polygons reaching past ±85° keep one class there.
pub struct WaterWindow {
    px: i64,
    grid: i64,
    /// First pixel and size of the window (clamped to the world).
    i0: i64,
    j0: i64,
    w: usize,
    h: usize,
    /// Summed-area table of water pixels, `(w + 1) × (h + 1)`.
    sat: Vec<u32>,
    /// Water pixels as rectangles `[x0, y0, x1, y1]` in fixed units (row runs merged downwards).
    rects: Vec<[i32; 4]>,
}

/// Where a polygon lies on the water mask.
#[derive(Debug, PartialEq)]
pub enum Split {
    Land,
    Water,
    /// Polygons over water and over land (exterior ring first, then holes).
    Parts { water: Vec<Rings>, land: Vec<Rings> },
}

impl WaterWindow {
    /// The window of a grid of `grid` pixels across a world of `world` fixed units that covers
    /// `bbox = [x0, y0, x1, y1]`. `fill(i0, j0, w, h, out)` sets `out[r·w + k]` when pixel
    /// `(i0 + k, j0 + r)` is water.
    pub fn new<F: FnOnce(i64, i64, usize, usize, &mut [bool]) -> Result<()>>(bbox: [i32; 4], world: i64, grid: i64, fill: F) -> Result<WaterWindow> {
        let px = world / grid;
        let cell = |v: i32| (v as i64).div_euclid(px).clamp(0, grid - 1);
        let (i0, j0) = (cell(bbox[0]), cell(bbox[1]));
        let (w, h) = ((cell(bbox[2]) - i0 + 1) as usize, (cell(bbox[3]) - j0 + 1) as usize);
        let (bx0, by0, bx1, by1) = (bbox[0] as i64, bbox[1] as i64, bbox[2] as i64, bbox[3] as i64);
        // Pixel edge → fixed units, stretching the world's outer edges over the bbox.
        let ex = |i: i64| if i <= 0 { bx0.min(0) } else if i >= grid { bx1.max(grid * px) } else { i * px };
        let ey = |j: i64| if j <= 0 { by0.min(0) } else if j >= grid { by1.max(grid * px) } else { j * px };
        let mut sat = vec![0u32; (w + 1) * (h + 1)];
        let mut rects = Vec::new();
        let mut mask = vec![false; w * h];
        fill(i0, j0, w, h, &mut mask)?;
        // Open rectangles: (first column, end column, first row).
        let mut open: Vec<(usize, usize, i64)> = Vec::new();
        let close = |rects: &mut Vec<[i32; 4]>, (a, b, top): (usize, usize, i64), bottom: i64| {
            rects.push([ex(i0 + a as i64) as i32, ey(top) as i32, ex(i0 + b as i64) as i32, ey(bottom) as i32]);
        };
        for r in 0..h {
            let j = j0 + r as i64;
            let row = &mask[r * w..(r + 1) * w];
            let mut acc = 0u32;
            for (k, &v) in row.iter().enumerate() {
                acc += v as u32;
                sat[(r + 1) * (w + 1) + k + 1] = sat[r * (w + 1) + k + 1] + acc;
            }
            let mut runs = Vec::new();
            let mut k = 0;
            while k < w {
                if row[k] {
                    let a = k;
                    while k < w && row[k] {
                        k += 1;
                    }
                    runs.push((a, k));
                } else {
                    k += 1;
                }
            }
            let mut next = Vec::with_capacity(runs.len());
            for &(a, b) in &runs {
                match open.iter().position(|o| o.0 == a && o.1 == b) {
                    Some(p) => next.push(open.swap_remove(p)),
                    None => next.push((a, b, j)),
                }
            }
            for o in open.drain(..) {
                close(&mut rects, o, j);
            }
            open = next;
        }
        for o in open.drain(..) {
            close(&mut rects, o, j0 + h as i64);
        }
        Ok(WaterWindow { px, grid, i0, j0, w, h, sat, rects })
    }

    /// The window of the HYPSO water mask (`water`: values that are water) on the Mercator grid
    /// of `smp`'s level, for a world of `world` fixed units.
    pub fn sample(bbox: [i32; 4], world: i64, smp: &mut Sampler, water: &[bool; 256]) -> Result<WaterWindow> {
        const BLOCK: usize = 16;
        let grid = smp.mercator_grid() as i64;
        let dlon = 360.0 / grid as f64;
        WaterWindow::new(bbox, world, grid, |i0, j0, w, h, out| {
            for by in (0..h).step_by(BLOCK) {
                for bx in (0..w).step_by(BLOCK) {
                    let (bw, bh) = (BLOCK.min(w - bx), BLOCK.min(h - by));
                    let j = j0 + by as i64;
                    let lat = |r: usize| geo::y_to_lat((j as f64 + r as f64 + 0.5) / grid as f64);
                    smp.water_block(lat, (i0 + bx as i64) as f64 * dlon - 180.0, dlon, bw, bh, water, &mut out[by * w + bx..], w)?;
                }
            }
            Ok(())
        })
    }

    /// Water pixels in the window's pixel ranges `[ia, ib) × [ja, jb)` (window-relative).
    fn count(&self, ia: usize, ib: usize, ja: usize, jb: usize) -> u32 {
        let s = |i: usize, j: usize| self.sat[j * (self.w + 1) + i];
        s(ib, jb) + s(ia, ja) - s(ia, jb) - s(ib, ja)
    }

    /// Splits a polygon (exterior ring first, then holes; even-odd) into its parts over water and
    /// over land. Polygons on one class only are returned as [`Split::Land`] / [`Split::Water`].
    pub fn split(&self, rings: &[Vec<FP>]) -> Split {
        let Some(ext) = rings.first().filter(|r| r.len() >= 3) else { return Split::Land };
        let mut b = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for p in ext {
            b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
        }
        let rel = |v: i32, o: i64, n: usize| ((v as i64).div_euclid(self.px).clamp(0, self.grid - 1) - o).clamp(0, n as i64 - 1) as usize;
        if self.w == 0 || self.h == 0 {
            return Split::Land;
        }
        let (ia, ib) = (rel(b[0], self.i0, self.w), rel(b[2], self.i0, self.w) + 1);
        let (ja, jb) = (rel(b[1], self.j0, self.h), rel(b[3], self.j0, self.h) + 1);
        let n = self.count(ia, ib, ja, jb);
        if n == 0 {
            return Split::Land;
        }
        if n as usize == (ib - ia) * (jb - ja) {
            return Split::Water;
        }
        let pt = |&(x, y): &FP| IntPoint::new(x, y);
        let subj: Vec<Vec<IntPoint>> = rings.iter().filter(|r| r.len() >= 3).map(|r| r.iter().map(pt).collect()).collect();
        let clip: Vec<Vec<IntPoint>> = self
            .rects
            .iter()
            .filter(|r| r[0] < b[2] && r[2] > b[0] && r[1] < b[3] && r[3] > b[1])
            .map(|r| vec![IntPoint::new(r[0], r[1]), IntPoint::new(r[2], r[1]), IntPoint::new(r[2], r[3]), IntPoint::new(r[0], r[3])])
            .collect();
        let mut ov: Overlay<i32> = Overlay::from_subj_and_clip(&subj, &clip);
        let Some(graph) = ov.build_graph_view(FillRule::EvenOdd) else { return Split::Land };
        let mut buf = BooleanExtractionBuffer::default();
        let back = |shapes: Vec<Vec<Vec<IntPoint>>>| -> Vec<Rings> {
            shapes.into_iter().map(|s| s.into_iter().map(|c| c.into_iter().map(|p| (p.x, p.y)).collect()).collect()).collect()
        };
        let water = back(graph.extract_shapes(OverlayRule::Intersect, &mut buf));
        let land = back(graph.extract_shapes(OverlayRule::Difference, &mut buf));
        match (water.is_empty(), land.is_empty()) {
            (true, _) => Split::Land,
            (false, true) => Split::Water,
            (false, false) => Split::Parts { water, land },
        }
    }
}

/// Area of a polygon (exterior minus holes), in squared fixed units.
pub fn polygon_area(rings: &[Vec<FP>]) -> f64 {
    let ring = |r: &Vec<FP>| {
        let n = r.len();
        (0..n).map(|i| r[i].0 as f64 * r[(i + 1) % n].1 as f64 - r[(i + 1) % n].0 as f64 * r[i].1 as f64).sum::<f64>().abs() / 2.0
    };
    let mut it = rings.iter();
    it.next().map_or(0.0, ring) - it.map(ring).sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_and_masks() {
        // HYPSO: water kind 0 → class 1 (bit 2), land kind 2 → class 4 (bit 1).
        assert_eq!((pixel_class(0), pixel_class(2), pixel_class(4)), (1, 4, 0));
        assert_eq!((class_bit(1), class_bit(4), class_bit(0), class_bit(5)), (2, 1, 16, 0));
        // Pen 358 "Sovereign - boundary" (over 0,1,0,0,0) and pen 442 (1,0,1,0,1).
        let s = SurfaceClasses { water: 1, land: 4, land_luminance: 200.0 };
        assert_eq!(s.pen_sides(pen_mask(&[0, 1, 0, 0, 0])), (true, false));
        assert_eq!(s.pen_sides(pen_mask(&[1, 0, 1, 0, 1])), (false, true));
        assert_eq!(pen_mask(&[1, 1, 1, 1, 1]), ALL_BITS);
        // Satellite-like land of kind 4 (class 0, bit 16).
        let sat = SurfaceClasses { water: 1, land: 0, land_luminance: 90.0 };
        assert_eq!(sat.pen_sides(pen_mask(&[1, 0, 1, 0, 1])), (false, true));
    }

    #[test]
    fn line_split_at_coast() {
        // Water west of 10° E.
        let runs = split_line(&[(0.0, 0.0), (0.0, 20.0), (1.0, 20.0)], 1.0, |_, lon| Ok(lon < 10.0)).unwrap();
        assert_eq!(runs.len(), 2);
        assert!(runs[0].0 && !runs[1].0);
        let x = runs[0].1.last().unwrap().1;
        assert!((x - 10.0).abs() < 0.01, "{x}");
        assert_eq!(runs[1].1.first(), runs[0].1.last());
        assert_eq!(runs[1].1.last(), Some(&(1.0, 20.0)));
        // A narrow strait inside one long segment is found by the sampling step.
        let runs = split_line(&[(0.0, 0.0), (0.0, 10.0)], 0.5, |_, lon| Ok((4.0..5.0).contains(&lon))).unwrap();
        assert_eq!(runs.iter().map(|r| r.0).collect::<Vec<_>>(), [false, true, false]);
    }

    /// A synthetic world of 4096 fixed units on a 64-pixel grid (64 units per pixel).
    fn window(bbox: [i32; 4], is_water: impl Fn(i64, i64) -> bool) -> WaterWindow {
        WaterWindow::new(bbox, 4096, 64, |i0, j0, w, _, out| {
            for (k, o) in out.iter_mut().enumerate() {
                *o = is_water(i0 + (k % w) as i64, j0 + (k / w) as i64);
            }
            Ok(())
        })
        .unwrap()
    }

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<FP> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    fn bbox(polys: &[Rings]) -> [i32; 4] {
        let mut b = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        for p in polys.iter().flatten().flatten() {
            b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
        }
        b
    }

    #[test]
    fn polygon_split_at_coast() {
        // Water west of pixel column 20 (x < 1280).
        let poly = vec![rect(1000, 1000, 2000, 2000), rect(1200, 1400, 1400, 1600)];
        let win = window([1000, 1000, 2000, 2000], |i, _| i < 20);
        let Split::Parts { water, land } = win.split(&poly) else { panic!("not split") };
        assert_eq!(bbox(&water), [1000, 1000, 1280, 2000]);
        assert_eq!(bbox(&land), [1280, 1000, 2000, 2000]);
        // The hole straddles the coast: each side keeps its share of it.
        let area = |ps: &[Rings]| ps.iter().map(|p| polygon_area(p)).sum::<f64>();
        assert_eq!(area(&water), 280.0 * 1000.0 - 80.0 * 200.0);
        assert_eq!(area(&land), 720.0 * 1000.0 - 120.0 * 200.0);
        assert_eq!(area(&water) + area(&land), polygon_area(&poly));
    }

    #[test]
    fn polygon_on_one_side_or_around_a_lake() {
        let lake = |i: i64, j: i64| (20..22).contains(&i) && (20..23).contains(&j);
        let win = window([0, 0, 4095, 4095], lake);
        // The lake's 2 × 3 pixels are one rectangle.
        assert_eq!(win.rects, vec![[1280, 1280, 1408, 1472]]);
        assert_eq!(win.split(&[rect(0, 0, 1000, 1000)]), Split::Land);
        assert_eq!(win.split(&[rect(1290, 1290, 1400, 1400)]), Split::Water);
        // A polygon around the lake: the land part gets the lake as a hole.
        let Split::Parts { water, land } = win.split(&[rect(1000, 1000, 2000, 2000)]) else { panic!("not split") };
        assert_eq!((water.len(), land.len(), land[0].len()), (1, 1, 2));
        assert_eq!(polygon_area(&water[0]), 128.0 * 192.0);
        // A bbox that touches water pixels while the polygon does not: kept whole.
        let tri = vec![vec![(1000, 1000), (1300, 1000), (1000, 1300)]];
        assert_eq!(win.split(&tri), Split::Land);
    }

    #[test]
    fn staircase_and_world_edges() {
        // Diagonal coast: water where i > j. Rows merge only where runs repeat.
        let win = window([0, 0, 511, 511], |i, j| i > j);
        assert_eq!(win.rects.len(), 7);
        let Split::Parts { water, land } = win.split(&[rect(0, 0, 512, 512)]) else { panic!("not split") };
        let area = |ps: &[Rings]| ps.iter().map(|p| polygon_area(p)).sum::<f64>();
        assert_eq!(area(&water), 64.0 * 64.0 * 28.0);
        assert_eq!(area(&land), 512.0 * 512.0 - area(&water));
        // Past the top of the world (y < 0) the first row's class continues.
        let win = window([0, -500, 4095, 100], |_, j| j == 0);
        assert_eq!(win.split(&[rect(100, -500, 200, 30)]), Split::Water);
        let Split::Parts { water, .. } = win.split(&[rect(100, -500, 200, 100)]) else { panic!("not split") };
        assert_eq!(bbox(&water), [100, -500, 200, 64]);
    }

    #[test]
    fn sphere_lattice() {
        let p = sphere_points(1000);
        let north = p.iter().filter(|q| q.0 > 0.0).count();
        assert!((480..=520).contains(&north));
    }
}
