//! `.3lt` light files (spec §7.2): point lights with precomputed face-corner lists, and (v3)
//! per-corner vertex paint.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::{Reader, Vec3, c_string};

const NAME_LEN: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct LightFile {
    /// 1, 2 or 3 (every shipped file is 3).
    pub version: u32,
    pub lights: Vec<Light>,
    /// Version 3 only.
    pub paint: Vec<Paint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Light {
    pub position: Vec3,
    /// Squared range: a vertex is lit only when |L−V|² < range2.
    pub range2: f32,
    /// D3DCOLOR `0xAARRGGBB`.
    pub colour: u32,
    /// Text before the NUL of the 256-byte buffer, exactly (may carry trailing spaces).
    pub name: String,
    /// `(object << 16) | face`; 0xFFFF in either half means "skip".
    pub face_refs: Vec<u32>,
    /// Per entry, the face's vertex slots this light reaches (bit k = slot k). `None` in version 1
    /// files, where every mask counts as all ones.
    pub vertex_masks: Option<Vec<u32>>,
}

/// One face reference of a light list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FaceRef {
    pub object: u16,
    pub face: u16,
}

impl FaceRef {
    pub fn from_raw(v: u32) -> Option<FaceRef> {
        let (object, face) = ((v >> 16) as u16, v as u16);
        (object != 0xFFFF && face != 0xFFFF).then_some(FaceRef { object, face })
    }
}

impl Light {
    /// `(face, vertex mask)` pairs, skipping 0xFFFF entries.
    pub fn entries(&self) -> impl Iterator<Item = (FaceRef, u32)> + '_ {
        self.face_refs.iter().enumerate().filter_map(|(i, &raw)| {
            let mask = self.vertex_masks.as_ref().map_or(u32::MAX, |m| m[i]);
            FaceRef::from_raw(raw).map(|f| (f, mask))
        })
    }

    pub fn rgb(&self) -> [u8; 3] {
        [(self.colour >> 16) as u8, (self.colour >> 8) as u8, self.colour as u8]
    }

    /// Light contribution factor at a vertex (spec §7.2.3): `(1 − d²/range2) · (n·L̂)`, or `None`
    /// when out of range or facing away.
    pub fn factor(&self, vertex: Vec3, normal: Vec3) -> Option<f32> {
        let d = [self.position[0] - vertex[0], self.position[1] - vertex[1], self.position[2] - vertex[2]];
        let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if d2 >= self.range2 || d2 == 0.0 {
            return None;
        }
        let len = d2.sqrt();
        let ndotl = (normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2]) / len;
        (ndotl > 0.0).then(|| (1.0 - d2 / self.range2) * ndotl)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Paint {
    /// D3DCOLOR forced onto the face corner.
    pub colour: u32,
    pub vertex_slot: i32,
    pub face: i32,
    /// Mesh name (text before the NUL).
    pub object: String,
}

impl LightFile {
    pub fn entry_count(&self) -> usize {
        self.lights.iter().map(|l| l.face_refs.len()).sum()
    }

    /// Light by script id: names upper-cased and compared exactly, first match (spec §7.1).
    pub fn by_script_name(&self, id: &str) -> Option<(usize, &Light)> {
        self.lights.iter().enumerate().find(|(_, l)| l.name.eq_ignore_ascii_case(id))
    }
}

pub fn parse(data: &[u8]) -> Result<LightFile> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    ensure!((1..=3).contains(&version), "unsupported light file version {version}");
    let n = r.count(280)?;
    let mut lights = Vec::with_capacity(n);
    for i in 0..n {
        let at = r.pos();
        let position = r.vec3()?;
        let range2 = r.f32()?;
        let colour = r.u32()?;
        let name = c_string(r.bytes(NAME_LEN)?);
        let count = r.count(4).with_context(|| format!("light {i} at {at}"))?;
        let face_refs = (0..count).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
        let vertex_masks = if version >= 2 { Some((0..count).map(|_| r.u32()).collect::<Result<Vec<_>>>()?) } else { None };
        lights.push(Light { position, range2, colour, name, face_refs, vertex_masks });
    }
    let mut paint = Vec::new();
    if version == 3 {
        let np = r.count(268)?;
        for _ in 0..np {
            paint.push(Paint { colour: r.u32()?, vertex_slot: r.i32()?, face: r.i32()?, object: c_string(r.bytes(NAME_LEN)?) });
        }
    }
    r.finish("light file")?;
    Ok(LightFile { version, lights, paint })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_buf(s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        b.resize(NAME_LEN, 0xCD); // junk after the NUL
        b
    }

    fn sample() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&3u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        for f in [1.0f32, 2.0, 3.0, 4.0] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&0xFF53_1E01u32.to_le_bytes());
        v.extend(name_buf("_TORCH1 "));
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&0x033D_0025u32.to_le_bytes());
        v.extend_from_slice(&0xFFFF_0001u32.to_le_bytes());
        v.extend_from_slice(&6u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&0xFF00_FF00u32.to_le_bytes());
        v.extend_from_slice(&2i32.to_le_bytes());
        v.extend_from_slice(&7i32.to_le_bytes());
        v.extend(name_buf("Box08ff"));
        v
    }

    #[test]
    fn parses_v3() {
        let f = parse(&sample()).unwrap();
        assert_eq!(f.entry_count(), 2);
        let l = &f.lights[0];
        assert_eq!(l.name, "_TORCH1 ");
        assert_eq!(l.rgb(), [0x53, 0x1E, 0x01]);
        assert_eq!(l.entries().collect::<Vec<_>>(), vec![(FaceRef { object: 829, face: 37 }, 6)]);
        assert!(f.by_script_name("_TORCH1").is_none(), "trailing space is significant");
        assert!(f.by_script_name("_torch1 ").is_some());
        assert_eq!(f.paint[0].object, "Box08ff");
        assert_eq!((f.paint[0].vertex_slot, f.paint[0].face), (2, 7));
        // Range2 4 → at distance 1 along the normal: (1 − 1/4)·1.
        assert_eq!(l.factor([1.0, 2.0, 2.0], [0.0, 0.0, 1.0]), Some(0.75));
        assert_eq!(l.factor([1.0, 2.0, 2.0], [0.0, 0.0, -1.0]), None);
        let mut long = sample();
        long.push(0);
        assert!(parse(&long).is_err());
    }
}
