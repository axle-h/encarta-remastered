//! `.3wm` walk maps (spec §6.2): ground, collision, hotspot and visibility cells.
//!
//! Visibility entries are sized by the face counts of the tour's `.x` meshes, so parsing needs
//! those counts (see [`super::xfile::XFile::face_counts`]).

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use super::{Reader, Vec3, cp1252};

#[derive(Debug, Clone, Serialize)]
pub struct WalkMap {
    /// 2 or 3 (every shipped file is 3).
    pub version: u32,
    /// Grid domain: min x,y,z then max x,y,z (not tight).
    pub bbox: [f32; 6],
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    /// Convex polygons (3 or 4 vertices) with real Y; only their XZ projection places the cell.
    pub polygons: Vec<Vec<Vec3>>,
    pub data: CellData,
}

#[derive(Debug, Clone, Serialize)]
pub enum CellData {
    /// Kind 2: potentially visible faces from this cell.
    Visibility(Vec<VisibilityEntry>),
    /// Kind 3: walls.
    Collision { level: f32, ignored_v2: Vec<u32>, segments: Vec<Segment> },
    /// Kind 4: ground plane `a·x + b·y + c·z + d = 0` (`None` in version 2 files).
    Ground(Option<[f32; 4]>),
    /// Kind 5: enter/exit script strings.
    Hotspot { enter: HotspotScript, exit: HotspotScript },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CellKind {
    Visibility = 2,
    Collision = 3,
    Ground = 4,
    Hotspot = 5,
}

impl CellData {
    pub fn kind(&self) -> CellKind {
        match self {
            CellData::Visibility(_) => CellKind::Visibility,
            CellData::Collision { .. } => CellKind::Collision,
            CellData::Ground(_) => CellKind::Ground,
            CellData::Hotspot { .. } => CellKind::Hotspot,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub top: f32,
    pub climbable: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisibilityEntry {
    /// 0-based mesh index in `.x` file order.
    pub mesh: u16,
    /// LSB-first face mask; bit j set = face j potentially visible.
    pub bits: Vec<u8>,
}

impl VisibilityEntry {
    pub fn face_visible(&self, face: usize) -> bool {
        self.bits.get(face >> 3).is_some_and(|b| b & (1 << (face & 7)) != 0)
    }

    pub fn visible_faces(&self, face_count: usize) -> impl Iterator<Item = usize> + '_ {
        (0..face_count).filter(|&j| self.face_visible(j))
    }
}

/// A hotspot cell's enter or exit string, split into the authored text and authoring junk.
///
/// The writer saved uninitialised buffer bytes after many strings. The authored text is printable
/// ASCII. Rule (spec §6.2): a string that is all printable ASCII (0x20–0x7E) is kept whole, even
/// without a final `;`; otherwise the authored part is its printable prefix up to and including
/// the prefix's last `;`, and the rest is junk.
#[derive(Debug, Clone, Serialize)]
pub struct HotspotScript {
    /// The authored statement list (what a converter should run).
    pub script: String,
    /// Authoring junk (decoded as cp1252). The original passes it to the interpreter, where it
    /// becomes malformed statements that fail silently.
    pub junk: String,
    /// Length of `junk` in bytes.
    pub junk_bytes: usize,
}

impl HotspotScript {
    pub fn from_bytes(b: &[u8]) -> Self {
        let printable = b.iter().position(|&c| !(0x20..=0x7E).contains(&c)).unwrap_or(b.len());
        let cut = if printable == b.len() { b.len() } else { b[..printable].iter().rposition(|&c| c == b';').map_or(0, |i| i + 1) };
        HotspotScript { script: cp1252(&b[..cut]), junk: cp1252(&b[cut..]), junk_bytes: b.len() - cut }
    }

    /// The full original string (`script` + `junk`), as the original interpreter sees it.
    pub fn original(&self) -> String {
        format!("{}{}", self.script, self.junk)
    }

    pub fn is_empty(&self) -> bool {
        self.script.is_empty() && self.junk_bytes == 0
    }
}

impl WalkMap {
    pub fn count(&self, kind: CellKind) -> usize {
        self.cells.iter().filter(|c| c.data.kind() == kind).count()
    }

    pub fn polygon_count(&self) -> usize {
        self.cells.iter().map(|c| c.polygons.len()).sum()
    }

    /// Hotspot cells as `(cell index, enter, exit)`.
    pub fn hotspots(&self) -> impl Iterator<Item = (usize, &HotspotScript, &HotspotScript)> {
        self.cells.iter().enumerate().filter_map(|(i, c)| match &c.data {
            CellData::Hotspot { enter, exit } => Some((i, enter, exit)),
            _ => None,
        })
    }

    /// Grid coordinates of a point on the 4096×4096 lookup grid (spec §6.3), or `None` when the
    /// point is "nowhere".
    pub fn grid_cell(&self, x: f32, z: f32) -> Option<(u32, u32)> {
        let [minx, _, minz, maxx, _, maxz] = self.bbox;
        let (ex, ez) = (maxx - minx, maxz - minz);
        if ex <= 0.001 || ez <= 0.001 {
            return None;
        }
        let gx = ((x - minx) / ex * 4095.0).trunc();
        let gz = ((z - minz) / ez * 4095.0).trunc();
        ((0.0..=4095.0).contains(&gx) && (0.0..=4095.0).contains(&gz)).then_some((gx as u32, gz as u32))
    }

    /// First cell of `kind` (file order) with a polygon containing (x, z) in the XZ plane. This is
    /// the point-in-polygon approximation of the grid lookup (exact except within one grid unit of
    /// an edge).
    pub fn cell_at(&self, kind: CellKind, x: f32, z: f32) -> Option<usize> {
        self.cells.iter().position(|c| c.data.kind() == kind && c.polygons.iter().any(|p| contains_xz(p, x, z)))
    }
}

/// Point-in-convex-polygon in XZ, either winding, boundary inclusive.
pub fn contains_xz(poly: &[Vec3], x: f32, z: f32) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let (mut pos, mut neg) = (false, false);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let cross = (b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0]);
        if cross > 0.0 {
            pos = true;
        } else if cross < 0.0 {
            neg = true;
        }
        if pos && neg {
            return false;
        }
    }
    true
}

/// Parses a walk map. `face_counts[i]` is the face count of mesh `i` of the tour's `.x` file.
pub fn parse(data: &[u8], face_counts: &[u32]) -> Result<WalkMap> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    ensure!(version == 2 || version == 3, "unsupported walk map version {version}");
    let mut bbox = [0f32; 6];
    for v in &mut bbox {
        *v = r.f32()?;
    }
    let ncells = r.count(6)?;
    let mut cells = Vec::with_capacity(ncells);
    for i in 0..ncells {
        cells.push(cell(&mut r, version, face_counts).with_context(|| format!("cell {i}"))?);
    }
    r.finish("walk map")?;
    Ok(WalkMap { version, bbox, cells })
}

fn cell(r: &mut Reader, version: u32, face_counts: &[u32]) -> Result<Cell> {
    let at = r.pos();
    let kind = r.u32()?;
    let npolys = r.u16()? as usize;
    let mut polygons = Vec::with_capacity(npolys);
    for _ in 0..npolys {
        let nv = r.u16()? as usize;
        polygons.push((0..nv).map(|_| r.vec3()).collect::<Result<Vec<_>>>()?);
    }
    let data = match kind {
        4 => {
            if version >= 3 {
                CellData::Ground(Some([r.f32()?, r.f32()?, r.f32()?, r.f32()?]))
            } else {
                r.u32()?;
                CellData::Ground(None)
            }
        }
        5 => {
            let enter = HotspotScript::from_bytes(r.lpstr()?);
            let exit = HotspotScript::from_bytes(r.lpstr()?);
            CellData::Hotspot { enter, exit }
        }
        3 => {
            let level = r.f32()?;
            let ignored_v2 = if version == 2 {
                let m = r.count(4)?;
                (0..m).map(|_| r.u32()).collect::<Result<_>>()?
            } else {
                Vec::new()
            };
            let nseg = r.count(24)?;
            let mut segments = Vec::with_capacity(nseg);
            for _ in 0..nseg {
                segments.push(Segment { x1: r.f32()?, z1: r.f32()?, x2: r.f32()?, z2: r.f32()?, top: r.f32()?, climbable: r.u32()? });
            }
            CellData::Collision { level, ignored_v2, segments }
        }
        2 => {
            let n = r.u16()? as usize;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                let mesh = r.u16()?;
                let faces = *face_counts.get(mesh as usize).with_context(|| {
                    format!("visibility entry names mesh {mesh} but the model has {} meshes", face_counts.len())
                })?;
                entries.push(VisibilityEntry { mesh, bits: r.bytes((faces as usize).div_ceil(8))?.to_vec() });
            }
            CellData::Visibility(entries)
        }
        other => bail!("unknown cell kind {other} at offset {at}"),
    };
    Ok(Cell { polygons, data })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn put(v: &mut Vec<u8>, b: &[u8]) {
        v.extend_from_slice(b);
    }

    fn square(v: &mut Vec<u8>, y: f32) {
        put(v, &1u16.to_le_bytes());
        put(v, &4u16.to_le_bytes());
        for (x, z) in [(0.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            for c in [x, y, z] {
                put(v, &c.to_le_bytes());
            }
        }
    }

    /// A v3 map with one cell of each kind.
    pub fn sample() -> Vec<u8> {
        let mut v = Vec::new();
        put(&mut v, &3u32.to_le_bytes());
        for c in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
            put(&mut v, &c.to_le_bytes());
        }
        put(&mut v, &4u32.to_le_bytes());
        // ground
        put(&mut v, &4u32.to_le_bytes());
        square(&mut v, 0.5);
        for c in [0.0f32, 1.0, 0.0, -0.5] {
            put(&mut v, &c.to_le_bytes());
        }
        // hotspot with junk after the last ';'
        put(&mut v, &5u32.to_le_bytes());
        square(&mut v, 0.5);
        let enter = b"2D._L1.VISIBLE=1;T\xe8;\n";
        put(&mut v, &(enter.len() as u32).to_le_bytes());
        put(&mut v, enter);
        put(&mut v, &0u32.to_le_bytes());
        // collision
        put(&mut v, &3u32.to_le_bytes());
        square(&mut v, 0.5);
        put(&mut v, &0.25f32.to_le_bytes());
        put(&mut v, &1u32.to_le_bytes());
        for c in [0.0f32, 0.0, 1.0, 0.0, 1.0] {
            put(&mut v, &c.to_le_bytes());
        }
        put(&mut v, &0u32.to_le_bytes());
        // visibility: mesh 1 (10 faces -> 2 bytes)
        put(&mut v, &2u32.to_le_bytes());
        put(&mut v, &0u16.to_le_bytes());
        put(&mut v, &1u16.to_le_bytes());
        put(&mut v, &1u16.to_le_bytes());
        put(&mut v, &[0b0000_0101, 0b10]);
        v
    }

    #[test]
    fn parses_every_cell_kind() {
        let w = parse(&sample(), &[4, 10]).unwrap();
        assert_eq!(w.cells.len(), 4);
        assert_eq!([w.count(CellKind::Ground), w.count(CellKind::Hotspot), w.count(CellKind::Collision), w.count(CellKind::Visibility)], [1; 4]);
        assert_eq!(w.polygon_count(), 3);
        let (i, enter, exit) = w.hotspots().next().unwrap();
        assert_eq!(i, 1);
        assert_eq!(enter.script, "2D._L1.VISIBLE=1;");
        assert_eq!(enter.junk_bytes, 4, "junk may hold printable bytes and ';'");
        let clean = HotspotScript::from_bytes(b"CAMERA.HEIGHT=.16000");
        assert_eq!((clean.script.as_str(), clean.junk_bytes), ("CAMERA.HEIGHT=.16000", 0), "clean unterminated string is kept");
        assert!(exit.is_empty());
        let CellData::Visibility(e) = &w.cells[3].data else { panic!() };
        assert_eq!(e[0].visible_faces(10).collect::<Vec<_>>(), vec![0, 2, 9]);
        assert_eq!(w.cell_at(CellKind::Ground, 0.5, 0.5), Some(0));
        assert_eq!(w.cell_at(CellKind::Ground, 1.5, 0.5), None);
        assert_eq!(w.grid_cell(1.0, 0.0), Some((4095, 0)));
    }

    #[test]
    fn rejects_bad_maps() {
        assert!(parse(&sample(), &[4]).is_err(), "mesh index past the model");
        let mut v = sample();
        v.push(0);
        assert!(parse(&v, &[4, 10]).is_err(), "trailing byte");
        let mut v = sample();
        v[0] = 1;
        assert!(parse(&v, &[4, 10]).is_err(), "version 1");
    }
}
