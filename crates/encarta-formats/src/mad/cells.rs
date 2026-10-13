//! Spatial cell keys of the vector tables (mad.md §7.8, bands from `GeoFaceScale` §8.2).
//!
//! Each band covers the globe with `n × n` cells on each of its 8 octants. Inside a cell, points
//! have coordinates `x, y ∈ 0…1023` on a plain latitude/longitude grid of the octant.

use anyhow::{Result, ensure};
use serde::Serialize;

/// Largest in-cell coordinate.
pub const CELL_MAX: f64 = 1023.0;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Band {
    /// Map-scale denominators (1:max … 1:min), as stored.
    pub max_scale: u32,
    pub min_scale: u32,
    pub log2_cells: u16,
    /// Cells per octant edge.
    pub n: u32,
    /// First cell key of the band.
    pub first_key: u32,
}

impl Band {
    pub fn key_count(&self) -> u32 {
        8 * self.n * self.n
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CellSystem {
    pub bands: Vec<Band>,
}

/// One cell: band, octant (bit 0 west of ±90°… see `lat_lon`), row (latitude) and column.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Cell {
    pub band: usize,
    pub n: u32,
    pub octant: u32,
    pub row: u32,
    pub col: u32,
}

impl CellSystem {
    /// Parses `GeoFaceScale`: `u32 max_scale, u32 min_scale, u16 log2(n)` per band.
    pub fn parse(b: &[u8]) -> Result<CellSystem> {
        ensure!(b.len().is_multiple_of(10) && !b.is_empty(), "GeoFaceScale length {} is not a multiple of 10", b.len());
        let mut bands = Vec::new();
        let mut first: u64 = 0;
        for r in b.as_chunks::<10>().0 {
            let log2 = u16::from_le_bytes([r[8], r[9]]);
            ensure!(log2 <= 14, "GeoFaceScale: band with 2^{log2} cells");
            let n = 1u32 << log2;
            bands.push(Band {
                max_scale: u32::from_le_bytes(r[0..4].try_into().unwrap()),
                min_scale: u32::from_le_bytes(r[4..8].try_into().unwrap()),
                log2_cells: log2,
                n,
                first_key: first as u32,
            });
            first += 8 * (n as u64) * (n as u64);
            ensure!(first <= u32::MAX as u64, "GeoFaceScale: key space overflow");
        }
        Ok(CellSystem { bands })
    }

    /// One past the last valid key.
    pub fn key_end(&self) -> u32 {
        self.bands.last().map_or(0, |b| b.first_key + b.key_count())
    }

    pub fn cell(&self, key: u32) -> Option<Cell> {
        let band = self.bands.iter().rposition(|b| b.first_key <= key)?;
        let b = &self.bands[band];
        let rem = key - b.first_key;
        if rem >= b.key_count() {
            return None;
        }
        let nn = b.n * b.n;
        Some(Cell { band, n: b.n, octant: rem / nn, row: (rem % nn) / b.n, col: rem % b.n })
    }

    pub fn key(&self, c: &Cell) -> u32 {
        let b = &self.bands[c.band];
        b.first_key + c.octant * b.n * b.n + c.row * b.n + c.col
    }

    /// The cell of band `band` containing `lat`, `lon`.
    pub fn locate(&self, band: usize, lat: f64, lon: f64) -> Cell {
        let n = self.bands[band].n;
        let (octant, phi, lambda) = octant_angles(lat, lon);
        let delta = 90.0 / n as f64;
        let idx = |a: f64| ((a / delta).floor() as i64).clamp(0, n as i64 - 1) as u32;
        Cell { band, n, octant, row: idx(phi), col: idx(lambda) }
    }

    /// The band whose scale range contains the map-scale denominator `scale`.
    pub fn band_for_scale(&self, scale: u32) -> usize {
        self.bands.iter().position(|b| scale >= b.min_scale).unwrap_or(self.bands.len() - 1)
    }
}

/// Octant number and in-octant latitude φ / longitude λ (degrees, 0…90) of a point.
pub fn octant_angles(lat: f64, lon: f64) -> (u32, f64, f64) {
    let l = lon.to_radians();
    let (c, s) = (l.cos(), l.sin());
    let mut octant = 0;
    if c < 0.0 {
        octant |= 1;
    }
    if s < 0.0 {
        octant |= 2;
    }
    if lat < 0.0 {
        octant |= 4;
    }
    (octant, lat.abs(), s.abs().atan2(c.abs()).to_degrees())
}

impl Cell {
    fn delta(&self) -> f64 {
        90.0 / self.n as f64
    }

    /// Latitude and longitude (degrees) of in-cell coordinates `x, y` (0…1023).
    pub fn lat_lon(&self, x: f64, y: f64) -> (f64, f64) {
        let d = self.delta();
        let phi = self.row as f64 * d + y * d / CELL_MAX;
        let lambda = (self.col as f64 * d + x * d / CELL_MAX).to_radians();
        let sx = if self.octant & 1 != 0 { -1.0 } else { 1.0 };
        let sy = if self.octant & 2 != 0 { -1.0 } else { 1.0 };
        let sz = if self.octant & 4 != 0 { -1.0 } else { 1.0 };
        let lat = sz * phi;
        let lon = (sy * lambda.sin()).atan2(sx * lambda.cos()).to_degrees();
        (lat, lon)
    }

    /// In-cell coordinates of a point (may fall outside 0…1023 for points outside the cell).
    pub fn local(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (_, phi, lambda) = octant_angles(lat, lon);
        let d = self.delta();
        ((lambda - self.col as f64 * d) * CELL_MAX / d, (phi - self.row as f64 * d) * CELL_MAX / d)
    }

    /// Bounding box `(west, south, east, north)` in degrees (longitudes may wrap at ±180°).
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let corners = [(0.0, 0.0), (CELL_MAX, 0.0), (0.0, CELL_MAX), (CELL_MAX, CELL_MAX)];
        let pts: Vec<(f64, f64)> = corners.iter().map(|&(x, y)| self.lat_lon(x, y)).collect();
        let lat0 = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let lat1 = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let lon0 = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let lon1 = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        (lon0, lat0, lon1, lat1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(logs: &[u16]) -> CellSystem {
        let mut b = Vec::new();
        for (i, &l) in logs.iter().enumerate() {
            b.extend((1000 * (logs.len() - i) as u32).to_le_bytes());
            b.extend((1000 * (logs.len() - i - 1) as u32 + 1).to_le_bytes());
            b.extend(l.to_le_bytes());
        }
        CellSystem::parse(&b).unwrap()
    }

    #[test]
    fn keys_and_bands() {
        let s = system(&[0, 0, 1, 3]);
        assert_eq!(s.bands.iter().map(|b| b.first_key).collect::<Vec<_>>(), [0, 8, 16, 48]);
        assert_eq!(s.key_end(), 48 + 8 * 64);
        let c = s.cell(16 + 4 * 4 + 2 + 1).unwrap();
        assert_eq!((c.band, c.octant, c.row, c.col), (2, 4, 1, 1));
        assert_eq!(s.key(&c), 16 + 19);
        assert!(s.cell(s.key_end()).is_none());
    }

    #[test]
    fn octant_geometry() {
        let s = system(&[0, 2]);
        // Octant 0 (north, 0…90° E), first cell: corner at 0°/0°.
        let c = s.cell(8).unwrap();
        let (lat, lon) = c.lat_lon(0.0, 0.0);
        assert!(lat.abs() < 1e-12 && lon.abs() < 1e-12);
        let (lat, lon) = c.lat_lon(CELL_MAX, CELL_MAX);
        assert!((lat - 22.5).abs() < 1e-12 && (lon - 22.5).abs() < 1e-12);
        // Octant 7: south, west of Greenwich, beyond 90° W.
        let c = Cell { band: 1, n: 4, octant: 7, row: 1, col: 1 };
        let (lat, lon) = c.lat_lon(CELL_MAX / 2.0, CELL_MAX / 2.0);
        assert!((lat + 33.75).abs() < 1e-9 && (lon + 180.0 - 33.75).abs() < 1e-9, "{lat} {lon}");
        for &(lat, lon) in &[(12.5, -69.9), (-33.9, 151.2), (51.5, -0.1), (64.1, -150.0)] {
            let c = s.locate(1, lat, lon);
            let (x, y) = c.local(lat, lon);
            assert!((0.0..=CELL_MAX).contains(&x) && (0.0..=CELL_MAX).contains(&y));
            let back = c.lat_lon(x, y);
            assert!((back.0 - lat).abs() < 1e-9 && (back.1 - lon).abs() < 1e-9);
        }
    }
}
