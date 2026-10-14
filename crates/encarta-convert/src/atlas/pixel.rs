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

use super::raster::TileCache;
use anyhow::Result;
use encarta_formats::mad::octahedral::{pixel_of, vector_lat_lon};
use encarta_formats::mad::rmap::{self, Look, Raster};

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
}

impl<'r, 'a> Sampler<'r, 'a> {
    pub fn new(raster: &'r Raster<'a>, level: u32) -> Self {
        let level = level.min(raster.info().max_level);
        Sampler { cache: TileCache::new(raster, 64), level, side: rmap::image_side(level) as u32 }
    }

    pub fn value(&mut self, lat: f64, lon: f64) -> Result<u8> {
        let (row, col) = pixel_of(lat, lon, self.side);
        self.cache.pixel(self.level, row as usize, col as usize)
    }

    /// Size of one pixel in degrees (approximately; the octahedral pixels vary a little).
    pub fn pixel_degrees(&self) -> f64 {
        180.0 / self.side as f64
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

    #[test]
    fn sphere_lattice() {
        let p = sphere_points(1000);
        let north = p.iter().filter(|q| q.0 > 0.0).count();
        assert!((480..=520).contains(&north));
    }
}
