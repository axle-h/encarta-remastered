//! Mapbox Vector Tile (v2.1) encoder, written from the public specification: a tile is a list of
//! layers; each layer holds keys, values and features whose geometry is a command stream
//! (MoveTo 1, LineTo 2, ClosePath 7) of zigzag-encoded deltas in tile units.

use std::collections::HashMap;

pub const EXTENT: u32 = 4096;

pub const POINT: u32 = 1;
pub const LINESTRING: u32 = 2;
pub const POLYGON: u32 = 3;

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn key(out: &mut Vec<u8>, field: u32, wire: u32) {
    varint(out, ((field << 3) | wire) as u64);
}

fn bytes_field(out: &mut Vec<u8>, field: u32, b: &[u8]) {
    key(out, field, 2);
    varint(out, b.len() as u64);
    out.extend_from_slice(b);
}

fn zigzag(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Uint(u64),
    Int(i64),
    Bool(bool),
}

#[derive(Hash, PartialEq, Eq)]
enum ValueKey {
    Str(String),
    Uint(u64),
    Int(i64),
    Bool(bool),
}

impl Value {
    fn key(&self) -> ValueKey {
        match self {
            Value::Str(s) => ValueKey::Str(s.clone()),
            Value::Uint(v) => ValueKey::Uint(*v),
            Value::Int(v) => ValueKey::Int(*v),
            Value::Bool(v) => ValueKey::Bool(*v),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        match self {
            Value::Str(s) => bytes_field(&mut b, 1, s.as_bytes()),
            Value::Uint(v) => {
                key(&mut b, 5, 0);
                varint(&mut b, *v);
            }
            Value::Int(v) => {
                key(&mut b, 6, 0);
                varint(&mut b, ((v << 1) ^ (v >> 63)) as u64);
            }
            Value::Bool(v) => {
                key(&mut b, 7, 0);
                varint(&mut b, u64::from(*v));
            }
        }
        b
    }
}

/// Geometry command stream builder (tile coordinates).
#[derive(Default)]
pub struct Geometry {
    cmds: Vec<u32>,
    cursor: (i32, i32),
}

impl Geometry {
    fn command(&mut self, id: u32, count: u32) {
        self.cmds.push((id & 7) | (count << 3));
    }

    fn delta(&mut self, p: (i32, i32)) {
        self.cmds.push(zigzag(p.0 - self.cursor.0));
        self.cmds.push(zigzag(p.1 - self.cursor.1));
        self.cursor = p;
    }

    /// Points (one MoveTo with all of them).
    pub fn points(&mut self, pts: &[(i32, i32)]) {
        if pts.is_empty() {
            return;
        }
        self.command(1, pts.len() as u32);
        for &p in pts {
            self.delta(p);
        }
    }

    pub fn line(&mut self, pts: &[(i32, i32)]) {
        if pts.len() < 2 {
            return;
        }
        self.command(1, 1);
        self.delta(pts[0]);
        self.command(2, pts.len() as u32 - 1);
        for &p in &pts[1..] {
            self.delta(p);
        }
    }

    /// A ring without its closing point; the caller orients it (exterior: positive area).
    pub fn ring(&mut self, pts: &[(i32, i32)]) {
        if pts.len() < 3 {
            return;
        }
        self.command(1, 1);
        self.delta(pts[0]);
        self.command(2, pts.len() as u32 - 1);
        for &p in &pts[1..] {
            self.delta(p);
        }
        self.command(7, 1);
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

/// Twice the signed area by the surveyor's formula (positive = exterior in MVT, y down).
pub fn signed_area2(pts: &[(i32, i32)]) -> i64 {
    let n = pts.len();
    let mut s = 0i64;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        s += a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64;
    }
    s
}

pub struct Layer {
    name: String,
    keys: Vec<String>,
    key_index: HashMap<String, u32>,
    values: Vec<Value>,
    value_index: HashMap<ValueKey, u32>,
    features: Vec<Vec<u8>>,
}

impl Layer {
    pub fn new(name: &str) -> Layer {
        Layer { name: name.to_string(), keys: Vec::new(), key_index: HashMap::new(), values: Vec::new(), value_index: HashMap::new(), features: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn add(&mut self, id: Option<u64>, geom_type: u32, geometry: &Geometry, props: &[(&str, Value)]) {
        if geometry.is_empty() {
            return;
        }
        let mut tags = Vec::with_capacity(props.len() * 2);
        for (k, v) in props {
            let ki = match self.key_index.get(*k) {
                Some(&i) => i,
                None => {
                    let i = self.keys.len() as u32;
                    self.keys.push(k.to_string());
                    self.key_index.insert(k.to_string(), i);
                    i
                }
            };
            let vk = v.key();
            let vi = match self.value_index.get(&vk) {
                Some(&i) => i,
                None => {
                    let i = self.values.len() as u32;
                    self.values.push(v.clone());
                    self.value_index.insert(vk, i);
                    i
                }
            };
            tags.push(ki);
            tags.push(vi);
        }
        let mut f = Vec::new();
        if let Some(id) = id {
            key(&mut f, 1, 0);
            varint(&mut f, id);
        }
        let mut packed = Vec::new();
        for t in tags {
            varint(&mut packed, t as u64);
        }
        bytes_field(&mut f, 2, &packed);
        key(&mut f, 3, 0);
        varint(&mut f, geom_type as u64);
        let mut g = Vec::new();
        for c in &geometry.cmds {
            varint(&mut g, *c as u64);
        }
        bytes_field(&mut f, 4, &g);
        self.features.push(f);
    }

    fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        key(&mut b, 15, 0);
        varint(&mut b, 2);
        bytes_field(&mut b, 1, self.name.as_bytes());
        for f in &self.features {
            bytes_field(&mut b, 2, f);
        }
        for k in &self.keys {
            bytes_field(&mut b, 3, k.as_bytes());
        }
        for v in &self.values {
            bytes_field(&mut b, 4, &v.encode());
        }
        key(&mut b, 5, 0);
        varint(&mut b, EXTENT as u64);
        b
    }
}

/// Encodes the non-empty layers into one tile.
pub fn encode_tile(layers: &[Layer]) -> Vec<u8> {
    let mut b = Vec::new();
    for l in layers.iter().filter(|l| !l.features.is_empty()) {
        bytes_field(&mut b, 3, &l.encode());
    }
    b
}

// ---------------------------------------------------------------------------------------------
// A small decoder, for validation previews and tests.

#[allow(dead_code)] // decoded for validation and tests
pub struct DecodedFeature {
    pub id: Option<u64>,
    pub geom_type: u32,
    /// Rings or lines or point groups, in tile coordinates.
    pub parts: Vec<Vec<(i32, i32)>>,
    pub props: Vec<(String, Value)>,
}

#[allow(dead_code)]
pub struct DecodedLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<DecodedFeature>,
}

fn rd_varint(b: &[u8], p: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut s = 0;
    loop {
        let c = *b.get(*p)?;
        *p += 1;
        v |= u64::from(c & 0x7F) << s;
        if c & 0x80 == 0 {
            return Some(v);
        }
        s += 7;
    }
}

/// Iterates `(field, wire type, varint value or byte slice)` of one message.
fn fields(b: &[u8]) -> Option<Vec<(u32, u64, &[u8])>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < b.len() {
        let k = rd_varint(b, &mut p)?;
        let (field, wire) = ((k >> 3) as u32, k & 7);
        match wire {
            0 => out.push((field, rd_varint(b, &mut p)?, &b[0..0])),
            2 => {
                let l = rd_varint(b, &mut p)? as usize;
                out.push((field, 0, b.get(p..p + l)?));
                p += l;
            }
            1 => p += 8,
            5 => p += 4,
            _ => return None,
        }
    }
    Some(out)
}

fn packed(b: &[u8]) -> Option<Vec<u64>> {
    let mut p = 0;
    let mut v = Vec::new();
    while p < b.len() {
        v.push(rd_varint(b, &mut p)?);
    }
    Some(v)
}

pub fn decode_tile(b: &[u8]) -> Option<Vec<DecodedLayer>> {
    let mut layers = Vec::new();
    for (f, _, lb) in fields(b)? {
        if f != 3 {
            continue;
        }
        let mut name = String::new();
        let mut extent = 4096;
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut raw = Vec::new();
        for (lf, v, s) in fields(lb)? {
            match lf {
                1 => name = String::from_utf8_lossy(s).into_owned(),
                2 => raw.push(s),
                3 => keys.push(String::from_utf8_lossy(s).into_owned()),
                4 => {
                    let mut val = Value::Bool(false);
                    for (vf, vv, vs) in fields(s)? {
                        val = match vf {
                            1 => Value::Str(String::from_utf8_lossy(vs).into_owned()),
                            5 => Value::Uint(vv),
                            6 => Value::Int(((vv >> 1) as i64) ^ -((vv & 1) as i64)),
                            7 => Value::Bool(vv != 0),
                            _ => val,
                        };
                    }
                    values.push(val);
                }
                5 => extent = v as u32,
                _ => {}
            }
        }
        let mut features = Vec::new();
        for fb in raw {
            let (mut id, mut gt, mut tags, mut geom) = (None, 0, Vec::new(), Vec::new());
            for (ff, v, s) in fields(fb)? {
                match ff {
                    1 => id = Some(v),
                    2 => tags = packed(s)?,
                    3 => gt = v as u32,
                    4 => geom = packed(s)?,
                    _ => {}
                }
            }
            let mut parts: Vec<Vec<(i32, i32)>> = Vec::new();
            let (mut x, mut y) = (0i32, 0i32);
            let mut i = 0;
            while i < geom.len() {
                let (cmd, count) = ((geom[i] & 7) as u32, (geom[i] >> 3) as usize);
                i += 1;
                match cmd {
                    1 | 2 => {
                        for _ in 0..count {
                            let dx = geom[i] as u32;
                            let dy = geom[i + 1] as u32;
                            i += 2;
                            x += ((dx >> 1) as i32) ^ -((dx & 1) as i32);
                            y += ((dy >> 1) as i32) ^ -((dy & 1) as i32);
                            if cmd == 1 && (gt != POINT || parts.is_empty()) {
                                parts.push(Vec::new());
                            }
                            parts.last_mut()?.push((x, y));
                        }
                    }
                    7 => {}
                    _ => return None,
                }
            }
            let props = tags.chunks(2).filter_map(|t| Some((keys.get(t[0] as usize)?.clone(), values.get(*t.get(1)? as usize)?.clone()))).collect();
            features.push(DecodedFeature { id, geom_type: gt, parts, props });
        }
        layers.push(DecodedLayer { name, extent, features });
    }
    Some(layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_synthetic_geometry() {
        let mut points = Layer::new("points");
        let mut g = Geometry::default();
        g.points(&[(25, 17)]);
        points.add(Some(7), POINT, &g, &[("name", Value::Str("A".into())), ("class", Value::Uint(3))]);
        let mut polys = Layer::new("areas");
        let mut g = Geometry::default();
        // Exterior (positive area, clockwise on screen) with one hole (negative area).
        let outer = vec![(0, 0), (100, 0), (100, 100), (0, 100)];
        let hole = vec![(20, 20), (20, 80), (80, 80), (80, 20)];
        assert!(signed_area2(&outer) > 0 && signed_area2(&hole) < 0);
        g.ring(&outer);
        g.ring(&hole);
        polys.add(None, POLYGON, &g, &[("class", Value::Uint(3)), ("neg", Value::Int(-5)), ("b", Value::Bool(true))]);
        let mut lines = Layer::new("lines");
        let mut g = Geometry::default();
        g.line(&[(1, 1), (10, -3), (4000, 4100)]);
        g.line(&[(5, 5), (6, 6)]);
        lines.add(Some(1), LINESTRING, &g, &[]);
        let tile = encode_tile(&[points, polys, lines, Layer::new("empty")]);
        let layers = decode_tile(&tile).unwrap();
        assert_eq!(layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["points", "areas", "lines"]);
        let p = &layers[0].features[0];
        assert_eq!((p.id, p.geom_type, p.parts.clone()), (Some(7), POINT, vec![vec![(25, 17)]]));
        assert_eq!(p.props, vec![("name".into(), Value::Str("A".into())), ("class".into(), Value::Uint(3))]);
        let a = &layers[1].features[0];
        assert_eq!(a.parts, vec![outer, hole]);
        assert_eq!(a.props[1].1, Value::Int(-5));
        let l = &layers[2].features[0];
        assert_eq!(l.parts, vec![vec![(1, 1), (10, -3), (4000, 4100)], vec![(5, 5), (6, 6)]]);
        assert_eq!(layers[2].extent, EXTENT);
    }
}
