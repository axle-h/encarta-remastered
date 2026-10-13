//! `.3vp` view points (spec §7.7): sample points on the walkable surface with unit surface
//! normals. Known by observation only; no script loads them and the runtime has no handler.

use anyhow::Result;
use serde::Serialize;

use super::{Reader, Vec3};

#[derive(Debug, Clone, Serialize)]
pub struct ViewPoints {
    pub points: Vec<Vec3>,
    /// Unit vectors with positive y (same count as `points` on the disc).
    pub normals: Vec<Vec3>,
}

pub fn parse(data: &[u8]) -> Result<ViewPoints> {
    let mut r = Reader::new(data);
    let n = r.count(12)?;
    let points = (0..n).map(|_| r.vec3()).collect::<Result<_>>()?;
    let m = r.count(12)?;
    let normals = (0..m).map(|_| r.vec3()).collect::<Result<_>>()?;
    r.finish("view point file")?;
    Ok(ViewPoints { points, normals })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses() {
        let mut v = 1u32.to_le_bytes().to_vec();
        [1.0f32, 2.0, 3.0].iter().for_each(|f| v.extend_from_slice(&f.to_le_bytes()));
        v.extend_from_slice(&1u32.to_le_bytes());
        [0.0f32, 1.0, 0.0].iter().for_each(|f| v.extend_from_slice(&f.to_le_bytes()));
        let p = super::parse(&v).unwrap();
        assert_eq!((p.points[0], p.normals[0]), ([1.0, 2.0, 3.0], [0.0, 1.0, 0.0]));
        assert_eq!(v.len(), 8 + 24);
        v.pop();
        assert!(super::parse(&v).is_err());
    }
}
