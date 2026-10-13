//! `.3cl` point clouds: the statue impostors (spec §7.3). Version 2 and 3 points are identical;
//! version 3 adds a 4-byte trailer the runtime ignores.

use anyhow::{Result, ensure};
use serde::Serialize;

use super::Reader;

#[derive(Debug, Clone, Serialize)]
pub struct Cloud {
    pub version: u32,
    pub points: Vec<CloudPoint>,
    /// Version 3 only; ignored by the runtime (about max y − 1).
    pub trailer: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CloudPoint {
    /// Cloud-local integer coordinates (1 unit = 1 texel at scale 1).
    pub x: i16,
    pub y: i16,
    pub z: i16,
    /// Facing angle in degrees used to cull 500-point chunks, or NaN (always drawn).
    pub key: f32,
    /// RGB565.
    pub colour: u16,
}

impl CloudPoint {
    /// RGB565 expanded to 8 bits per channel (bit replication).
    pub fn rgb(&self) -> [u8; 3] {
        let c = self.colour;
        let (r, g, b) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
        [((r << 3) | (r >> 2)) as u8, ((g << 2) | (g >> 4)) as u8, ((b << 3) | (b >> 2)) as u8]
    }

    /// An all-zero padding record (position 0, colour 0), as found in `floral` and `wordcld`.
    pub fn is_zero(&self) -> bool {
        self.x == 0 && self.y == 0 && self.z == 0 && self.colour == 0
    }
}

impl Cloud {
    /// (min, max) of y.
    pub fn y_range(&self) -> Option<(i16, i16)> {
        let ys = self.points.iter().map(|p| p.y);
        Some((ys.clone().min()?, ys.max()?))
    }
}

pub fn parse(data: &[u8]) -> Result<Cloud> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    ensure!(version >= 2, "point cloud version {version} < 2");
    let n = r.count(16)?;
    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        let (x, y, z) = (r.i16()?, r.i16()?, r.i16()?);
        let pad1 = r.u16()?;
        let key = r.f32()?;
        let colour = r.u16()?;
        let pad2 = r.u16()?;
        ensure!(pad1 == 0 && pad2 == 0, "non-zero padding in point {} at {}", points.len(), r.pos() - 16);
        points.push(CloudPoint { x, y, z, key, colour });
    }
    let trailer = if version >= 3 { Some(r.u32()?) } else { None };
    r.finish("point cloud")?;
    Ok(Cloud { version, points, trailer })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(version: u32, pts: &[(i16, i16, i16, f32, u16)]) -> Vec<u8> {
        let mut v = version.to_le_bytes().to_vec();
        v.extend_from_slice(&(pts.len() as u32).to_le_bytes());
        for &(x, y, z, k, c) in pts {
            for q in [x, y, z, 0] {
                v.extend_from_slice(&q.to_le_bytes());
            }
            v.extend_from_slice(&k.to_le_bytes());
            v.extend_from_slice(&c.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
        }
        if version >= 3 {
            v.extend_from_slice(&41u32.to_le_bytes());
        }
        v
    }

    #[test]
    fn v2_and_v3() {
        let pts = [(-3, 42, 5, 12.5, 0xF800), (0, 0, 0, f32::NAN, 0)];
        let c3 = parse(&build(3, &pts)).unwrap();
        assert_eq!((c3.points.len(), c3.trailer), (2, Some(41)));
        assert_eq!(c3.points[0].rgb(), [255, 0, 0]);
        assert!(c3.points[1].key.is_nan() && c3.points[1].is_zero());
        assert_eq!(c3.y_range(), Some((0, 42)));
        let c2 = parse(&build(2, &pts)).unwrap();
        assert_eq!(c2.trailer, None);
        assert!(parse(&build(1, &pts)).is_err());
        let mut bad = build(2, &pts);
        bad.push(0);
        assert!(parse(&bad).is_err());
    }
}
