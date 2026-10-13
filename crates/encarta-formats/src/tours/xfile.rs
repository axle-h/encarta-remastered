//! Binary DirectX `.x` files, restricted to the subset the tours use (spec §5).
//!
//! The file is a 16-byte header followed by a token stream. Tokens are read into a small generic
//! object tree first, then the three templates that occur (`Header`, `Material`, `Mesh` with its
//! four children) are checked against the fixed shapes the original loader expects.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use super::{Reader, Vec3, cp1252};

// Token ids of the binary format.
const TOK_NAME: u16 = 0x01;
const TOK_STRING: u16 = 0x02;
const TOK_INTEGER: u16 = 0x03;
const TOK_GUID: u16 = 0x05;
const TOK_INT_LIST: u16 = 0x06;
const TOK_FLOAT_LIST: u16 = 0x07;
const TOK_OBRACE: u16 = 0x0A;
const TOK_CBRACE: u16 = 0x0B;
const TOK_SEMICOLON: u16 = 0x14;

#[derive(Debug, Clone, Serialize)]
pub struct XFile {
    /// `Header` object: major, minor, flags (1, 0, 0 on the disc).
    pub header: [u32; 3],
    /// Every top-level `Material`, in file order.
    pub materials: Vec<Material>,
    /// Index in `materials` of the material the original loader parses and throws away (the
    /// second top-level object when it is a Material: `xof_default` on the disc).
    pub discarded_material: Option<usize>,
    /// Every `Mesh`, in file order. Walk maps and lights address meshes by this index.
    pub meshes: Vec<Mesh>,
    /// Number of tokens in the stream: each `u16` token id counts once, except the `;` that
    /// terminates a STRING, which belongs to the STRING token.
    pub tokens: usize,
    /// Number of top-level objects (Header + Materials + Meshes).
    pub top_level_objects: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Material {
    pub name: Option<String>,
    /// RGBA.
    pub face_color: [f32; 4],
    pub power: f32,
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    /// The `TextureFilename` string, byte for byte (decoded as cp1252).
    pub texture: Option<String>,
}

impl Material {
    /// The 11 floats in file order (used for "identical material" merging, spec §5.4).
    pub fn floats(&self) -> [f32; 11] {
        let [r, g, b, a] = self.face_color;
        let [sr, sg, sb] = self.specular;
        let [er, eg, eb] = self.emissive;
        [r, g, b, a, self.power, sr, sg, sb, er, eg, eb]
    }

    /// True when both materials merge into one table entry: all floats bit-equal and the same
    /// texture name (case-sensitive).
    pub fn same_as(&self, other: &Material) -> bool {
        self.floats().map(f32::to_bits) == other.floats().map(f32::to_bits) && self.texture == other.texture
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Mesh {
    pub name: String,
    pub positions: Vec<Vec3>,
    /// Polygons as vertex index lists (3 or 4 entries on the disc).
    pub faces: Vec<Vec<u32>>,
    pub normals: Option<MeshNormals>,
    /// One UV per vertex. Values tile far outside 0..1 (sampler must REPEAT).
    pub uvs: Option<Vec<[f32; 2]>>,
    pub material_list: Option<MaterialList>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshNormals {
    pub normals: Vec<Vec3>,
    /// Per face, indices into `normals`; same shape as `Mesh::faces`.
    pub faces: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialList {
    /// The declared material count.
    pub material_count: u32,
    /// Either one index for the whole mesh, or one per face.
    pub face_indexes: Vec<u32>,
    pub materials: Vec<MaterialRef>,
}

#[derive(Debug, Clone, Serialize)]
pub enum MaterialRef {
    /// `{ name }` reference to a top-level Material (the only form on the disc).
    Named(String),
    Inline(Material),
}

impl MaterialList {
    /// Material slot used by face `face` (spec §5.4 "polygon material selection").
    pub fn slot_for_face(&self, face: usize, face_count: usize) -> Option<usize> {
        if self.face_indexes.len() == face_count {
            self.face_indexes.get(face).map(|&i| i as usize)
        } else if self.material_count == 1 {
            Some(0)
        } else {
            None
        }
    }
}

impl Mesh {
    /// `lod_<base>` meshes are low-detail stand-ins for `<base>` (case-sensitive prefix).
    pub fn lod_base(&self) -> Option<&str> {
        self.name.strip_prefix("lod_")
    }

    /// Triangles after fan triangulation from each face's first vertex.
    pub fn triangle_count(&self) -> usize {
        self.faces.iter().map(|f| f.len().saturating_sub(2)).sum()
    }

    /// Fan triangulation `(0,i,i+1)` in the stored (D3D clockwise) winding, as
    /// `(face index, [a, b, c])` with indices into `positions`.
    pub fn fan_triangles(&self) -> impl Iterator<Item = (usize, [u32; 3])> + '_ {
        self.faces.iter().enumerate().flat_map(|(fi, f)| (1..f.len().saturating_sub(1)).map(move |i| (fi, [f[0], f[i], f[i + 1]])))
    }

    /// Axis-aligned bounds of the positions, or `None` for an empty mesh.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        bounds(self.positions.iter())
    }

    /// Mean of the per-face centres (each the mean of the face's vertices). This is the object
    /// centre used for LOD switching, sprites and clouds (spec §7.11; definition still [VM]).
    pub fn centre(&self) -> Option<Vec3> {
        let mut sum = [0f64; 3];
        let mut n = 0usize;
        for f in &self.faces {
            if f.is_empty() {
                continue;
            }
            let mut c = [0f64; 3];
            for &i in f {
                let p = self.positions.get(i as usize)?;
                for k in 0..3 {
                    c[k] += p[k] as f64;
                }
            }
            for k in 0..3 {
                sum[k] += c[k] / f.len() as f64;
            }
            n += 1;
        }
        (n > 0).then(|| sum.map(|s| (s / n as f64) as f32))
    }
}

pub(crate) fn bounds<'a>(points: impl Iterator<Item = &'a Vec3>) -> Option<(Vec3, Vec3)> {
    let mut it = points.peekable();
    it.peek()?;
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in it {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    Some((lo, hi))
}

impl XFile {
    /// Top-level material by name for `{ref}` lookup (case-sensitive, first match, skipping the
    /// discarded one).
    pub fn material(&self, name: &str) -> Option<(usize, &Material)> {
        self.materials
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != self.discarded_material)
            .find(|(_, m)| m.name.as_deref() == Some(name))
    }

    /// Mesh by script id: case-insensitive, first match in file order (spec §7.1).
    pub fn mesh_by_script_name(&self, id: &str) -> Option<(usize, &Mesh)> {
        self.meshes.iter().enumerate().find(|(_, m)| m.name.eq_ignore_ascii_case(id))
    }

    /// Face count of every mesh, in file order: what walk-map visibility masks are sized by.
    pub fn face_counts(&self) -> Vec<u32> {
        self.meshes.iter().map(|m| m.faces.len() as u32).collect()
    }

    /// `(lod mesh index, partner index)` pairs; the partner is the first mesh whose name equals the
    /// rest after `lod_` (case-sensitive). `None` for an orphan.
    pub fn lod_pairs(&self) -> Vec<(usize, Option<usize>)> {
        let mut first_by_name = std::collections::HashMap::new();
        for (i, m) in self.meshes.iter().enumerate() {
            first_by_name.entry(m.name.as_str()).or_insert(i);
        }
        self.meshes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.lod_base().map(|b| (i, first_by_name.get(b).copied())))
            .collect()
    }

    /// Distinct texture names (case-sensitive) of every top-level material except the discarded
    /// one, in file order, each with whether some mesh uses a material carrying it.
    pub fn all_texture_names(&self) -> Vec<(String, bool)> {
        let used: std::collections::HashSet<String> = self.texture_names().into_iter().collect();
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (i, m) in self.materials.iter().enumerate() {
            if Some(i) == self.discarded_material {
                continue;
            }
            if let Some(t) = &m.texture
                && seen.insert(t.clone())
            {
                out.push((t.clone(), used.contains(t)));
            }
        }
        for t in self.texture_names() {
            if seen.insert(t.clone()) {
                out.push((t, true)); // inline materials
            }
        }
        out
    }

    /// Distinct texture names used by the meshes' materials (in first-use order).
    pub fn texture_names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for m in &self.meshes {
            for r in m.material_list.iter().flat_map(|l| &l.materials) {
                let mat = match r {
                    MaterialRef::Named(n) => self.material(n).map(|(_, m)| m),
                    MaterialRef::Inline(m) => Some(m),
                };
                if let Some(t) = mat.and_then(|m| m.texture.as_ref())
                    && seen.insert(t.clone())
                {
                    out.push(t.clone());
                }
            }
        }
        out
    }

    /// Number of groups of two or more identical materials (all floats and the texture name equal),
    /// which the engine's material table merges into one entry each.
    pub fn duplicate_material_groups(&self) -> usize {
        let mut groups: Vec<(&Material, usize)> = Vec::new();
        for m in &self.materials {
            match groups.iter_mut().find(|(g, _)| g.same_as(m)) {
                Some((_, n)) => *n += 1,
                None => groups.push((m, 1)),
            }
        }
        groups.iter().filter(|(_, n)| *n > 1).count()
    }

    /// Groups of mesh names that are equal ignoring ASCII case (scripts reach only the first).
    pub fn case_collision_groups(&self) -> usize {
        let mut by_upper: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for m in &self.meshes {
            *by_upper.entry(m.name.to_ascii_uppercase()).or_default() += 1;
        }
        by_upper.values().filter(|&&n| n > 1).count()
    }

    pub fn stats(&self) -> XStats {
        let mut s = XStats {
            tokens: self.tokens,
            top_level_objects: self.top_level_objects,
            materials: self.materials.len(),
            texture_filenames: self.materials.iter().filter(|m| m.texture.is_some()).count(),
            meshes: self.meshes.len(),
            ..Default::default()
        };
        for m in &self.meshes {
            s.vertices += m.positions.len();
            s.faces += m.faces.len();
            s.max_mesh_vertices = s.max_mesh_vertices.max(m.positions.len());
            for f in &m.faces {
                match f.len() {
                    3 => s.triangles_faces += 1,
                    4 => s.quad_faces += 1,
                    _ => s.other_faces += 1,
                }
                let mut sorted = f.clone();
                sorted.sort_unstable();
                sorted.dedup();
                if sorted.len() < f.len() {
                    s.degenerate_faces += 1;
                }
            }
            s.fan_triangles += m.triangle_count();
            if let Some(n) = &m.normals {
                s.with_normals += 1;
                if n.normals.len() == m.positions.len() {
                    s.normal_count_equals_vertex_count += 1;
                }
            }
            if let Some(uv) = &m.uvs {
                s.with_uvs += 1;
                s.non_finite_uvs += uv.iter().flatten().filter(|v| !v.is_finite()).count();
            }
            if let Some(l) = &m.material_list {
                s.with_material_list += 1;
                if l.face_indexes.len() == m.faces.len() {
                    s.per_face_material_lists += 1;
                } else if l.material_count == 1 && l.face_indexes.len() == 1 {
                    s.single_material_lists += 1;
                }
                if l.material_count > 1 {
                    s.multi_material_lists += 1;
                }
                s.max_materials_per_mesh = s.max_materials_per_mesh.max(l.material_count as usize);
            }
            if m.lod_base().is_some() {
                s.lod_meshes += 1;
            }
        }
        s.lod_orphans = self.lod_pairs().iter().filter(|(_, p)| p.is_none()).count();
        s.bounds = bounds(self.meshes.iter().flat_map(|m| &m.positions));
        s
    }
}

/// Whole-file counts (the self-check table of spec §5.3).
#[derive(Debug, Clone, Default, Serialize)]
pub struct XStats {
    pub tokens: usize,
    pub top_level_objects: usize,
    pub materials: usize,
    pub texture_filenames: usize,
    pub meshes: usize,
    pub with_normals: usize,
    pub with_uvs: usize,
    pub with_material_list: usize,
    pub vertices: usize,
    pub faces: usize,
    pub triangles_faces: usize,
    pub quad_faces: usize,
    pub other_faces: usize,
    pub degenerate_faces: usize,
    pub fan_triangles: usize,
    pub max_mesh_vertices: usize,
    pub normal_count_equals_vertex_count: usize,
    /// Non-finite UV components (u or v counted separately).
    pub non_finite_uvs: usize,
    /// `nFaceIndexes = 1` for a mesh of two or more faces (one material for the whole mesh).
    pub single_material_lists: usize,
    /// `nFaceIndexes = nFaces` (includes single-face meshes).
    pub per_face_material_lists: usize,
    /// Lists with two or more materials.
    pub multi_material_lists: usize,
    pub max_materials_per_mesh: usize,
    pub lod_meshes: usize,
    pub lod_orphans: usize,
    pub bounds: Option<(Vec3, Vec3)>,
}

// ------------------------------------------------------------------ token stream and object tree

enum Token<'a> {
    Name(&'a [u8]),
    Str(&'a [u8]),
    Ints(Vec<u32>),
    Floats(Vec<f32>),
    /// A GUID (16 bytes, value unused).
    Guid,
    Open,
    Close,
    Other(u16),
}

struct Tokens<'a> {
    r: Reader<'a>,
    count: usize,
    peeked: Option<(usize, Token<'a>)>,
}

impl<'a> Tokens<'a> {
    fn read(&mut self) -> Result<(usize, Token<'a>)> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        let at = self.r.pos();
        let id = self.r.u16()?;
        self.count += 1;
        let tok = match id {
            TOK_NAME => Token::Name(self.r.lpstr()?),
            TOK_STRING => {
                let s = self.r.lpstr()?;
                let term = self.r.u16()?;
                ensure!(term == TOK_SEMICOLON, "STRING at {at} not terminated by ';' (token {term:#x})");
                Token::Str(s)
            }
            TOK_INTEGER => Token::Ints(vec![self.r.u32()?]),
            TOK_GUID => {
                self.r.bytes(16)?;
                Token::Guid
            }
            TOK_INT_LIST => {
                let n = self.r.count(4)?;
                Token::Ints((0..n).map(|_| self.r.u32()).collect::<Result<_>>()?)
            }
            TOK_FLOAT_LIST => {
                let n = self.r.count(4)?;
                Token::Floats((0..n).map(|_| self.r.f32()).collect::<Result<_>>()?)
            }
            TOK_OBRACE => Token::Open,
            TOK_CBRACE => Token::Close,
            other => Token::Other(other),
        };
        Ok((at, tok))
    }

    fn peek_is_end(&self) -> bool {
        self.peeked.is_none() && self.r.at_end()
    }

    fn unread(&mut self, t: (usize, Token<'a>)) {
        self.peeked = Some(t);
    }
}

/// A generic object: `NAME(type) [NAME(instance)] [GUID] { items }`.
struct Object {
    at: usize,
    kind: String,
    name: Option<String>,
    items: Vec<Item>,
}

enum Item {
    Ints(Vec<u32>),
    Floats(Vec<f32>),
    Str(String),
    Ref(String),
    Child(Object),
}

fn parse_object(t: &mut Tokens) -> Result<Object> {
    let (at, tok) = t.read()?;
    let Token::Name(kind) = tok else { bail!("expected a template name at offset {at}") };
    let kind = cp1252(kind);
    let mut name = None;
    let mut next = t.read()?;
    if let (_, Token::Name(n)) = next {
        name = Some(cp1252(n));
        next = t.read()?;
    }
    if let (_, Token::Guid) = next {
        next = t.read()?;
    }
    let (open_at, Token::Open) = next else { bail!("{kind} at {at}: expected '{{'") };
    let mut items = Vec::new();
    loop {
        let (item_at, tok) = t.read()?;
        match tok {
            Token::Close => break,
            Token::Ints(v) => items.push(Item::Ints(v)),
            Token::Floats(v) => items.push(Item::Floats(v)),
            Token::Str(s) => items.push(Item::Str(cp1252(s))),
            Token::Open => {
                let (_, Token::Name(n)) = t.read()? else { bail!("reference at {item_at}: expected a name") };
                let (_, Token::Close) = t.read()? else { bail!("reference at {item_at}: expected '}}'") };
                items.push(Item::Ref(cp1252(n)));
            }
            tok @ Token::Name(_) => {
                t.unread((item_at, tok));
                items.push(Item::Child(parse_object(t)?));
            }
            Token::Guid => bail!("{kind} at {at}: stray GUID at {item_at}"),
            Token::Other(id) => bail!("{kind} at {at} (body from {open_at}): unsupported token {id:#x} at {item_at}"),
        }
    }
    Ok(Object { at, kind, name, items })
}

// ------------------------------------------------------------------ typed templates

fn ints<'a>(item: Option<&'a Item>, what: &str) -> Result<&'a [u32]> {
    match item {
        Some(Item::Ints(v)) => Ok(v),
        _ => bail!("{what}: expected an integer list"),
    }
}

fn floats<'a>(item: Option<&'a Item>, what: &str) -> Result<&'a [f32]> {
    match item {
        Some(Item::Floats(v)) => Ok(v),
        _ => bail!("{what}: expected a float list"),
    }
}

fn vec3s(f: &[f32]) -> Vec<Vec3> {
    f.as_chunks::<3>().0.to_vec()
}

/// `n, then per entry: count, idx...` with exactly `n` entries.
fn polygon_list(v: &[u32], what: &str) -> Result<Vec<Vec<u32>>> {
    let (&n, mut rest) = v.split_first().with_context(|| format!("{what}: empty face list"))?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let (&c, tail) = rest.split_first().with_context(|| format!("{what}: face {i} missing"))?;
        ensure!(tail.len() >= c as usize, "{what}: face {i} truncated");
        out.push(tail[..c as usize].to_vec());
        rest = &tail[c as usize..];
    }
    ensure!(rest.is_empty(), "{what}: {} extra integers after {n} faces", rest.len());
    Ok(out)
}

fn material(o: &Object) -> Result<Material> {
    let what = format!("Material {:?} at {}", o.name, o.at);
    let f = floats(o.items.first(), &what)?;
    ensure!(f.len() == 11, "{what}: {} floats, expected 11", f.len());
    let mut texture = None;
    for item in &o.items[1..] {
        match item {
            Item::Child(c) if c.kind == "TextureFilename" || c.kind == "TextureFileName" => {
                ensure!(texture.is_none(), "{what}: two TextureFilename children");
                let [Item::Str(s)] = c.items.as_slice() else { bail!("{what}: TextureFilename must hold one string") };
                texture = Some(s.clone());
            }
            _ => bail!("{what}: unexpected content"),
        }
    }
    Ok(Material {
        name: o.name.clone(),
        face_color: [f[0], f[1], f[2], f[3]],
        power: f[4],
        specular: [f[5], f[6], f[7]],
        emissive: [f[8], f[9], f[10]],
        texture,
    })
}

fn mesh(o: &Object) -> Result<Mesh> {
    let name = o.name.clone().with_context(|| format!("unnamed Mesh at {}", o.at))?;
    let what = format!("Mesh {name} at {}", o.at);
    let nv = ints(o.items.first(), &what)?;
    ensure!(nv.len() == 1, "{what}: vertex count list has {} entries", nv.len());
    let nv = nv[0] as usize;
    let pos = floats(o.items.get(1), &what)?;
    ensure!(pos.len() == 3 * nv, "{what}: {} position floats for {nv} vertices", pos.len());
    let faces = polygon_list(ints(o.items.get(2), &what)?, &what)?;
    for (i, f) in faces.iter().enumerate() {
        ensure!(f.iter().all(|&v| (v as usize) < nv), "{what}: face {i} indexes past {nv} vertices");
    }
    let mut mesh = Mesh { name, positions: vec3s(pos), faces, normals: None, uvs: None, material_list: None };
    for item in &o.items[3..] {
        let Item::Child(c) = item else { bail!("{what}: unexpected data after the face list") };
        let cw = format!("{what} {}", c.kind);
        match c.kind.as_str() {
            "MeshNormals" => {
                ensure!(mesh.normals.is_none(), "{cw}: duplicate");
                let n = ints(c.items.first(), &cw)?;
                ensure!(n.len() == 1, "{cw}: count list");
                let f = floats(c.items.get(1), &cw)?;
                ensure!(f.len() == 3 * n[0] as usize, "{cw}: float count");
                let nf = polygon_list(ints(c.items.get(2), &cw)?, &cw)?;
                ensure!(c.items.len() == 3, "{cw}: extra items");
                ensure!(nf.len() == mesh.faces.len(), "{cw}: {} faces vs {}", nf.len(), mesh.faces.len());
                for (a, b) in nf.iter().zip(&mesh.faces) {
                    ensure!(a.len() == b.len(), "{cw}: face size mismatch");
                    ensure!(a.iter().all(|&v| v < n[0]), "{cw}: normal index out of range");
                }
                mesh.normals = Some(MeshNormals { normals: vec3s(f), faces: nf });
            }
            "MeshTextureCoords" => {
                ensure!(mesh.uvs.is_none(), "{cw}: duplicate");
                let n = ints(c.items.first(), &cw)?;
                ensure!(n.len() == 1 && n[0] as usize == nv, "{cw}: count must equal the vertex count");
                let f = floats(c.items.get(1), &cw)?;
                ensure!(f.len() == 2 * nv && c.items.len() == 2, "{cw}: shape");
                mesh.uvs = Some(f.as_chunks::<2>().0.to_vec());
            }
            "MeshMaterialList" => {
                ensure!(mesh.material_list.is_none(), "{cw}: duplicate");
                let v = ints(c.items.first(), &cw)?;
                ensure!(v.len() >= 2, "{cw}: header");
                let (count, nidx) = (v[0], v[1] as usize);
                ensure!(v.len() == 2 + nidx, "{cw}: {} face indexes declared, list has {}", nidx, v.len() - 2);
                let mut materials = Vec::new();
                for m in &c.items[1..] {
                    materials.push(match m {
                        Item::Ref(n) => MaterialRef::Named(n.clone()),
                        Item::Child(mo) if mo.kind == "Material" => MaterialRef::Inline(material(mo)?),
                        _ => bail!("{cw}: expected material references"),
                    });
                }
                ensure!(materials.len() == count as usize, "{cw}: {} materials for count {count}", materials.len());
                ensure!(v[2..].iter().all(|&i| i < count), "{cw}: material index out of range");
                mesh.material_list = Some(MaterialList { material_count: count, face_indexes: v[2..].to_vec(), materials });
            }
            other => bail!("{what}: unsupported child template {other}"),
        }
    }
    Ok(mesh)
}

/// Parses a whole `.x` file. Fails on anything outside the subset (spec §5.1–5.2), including
/// trailing bytes.
pub fn parse(data: &[u8]) -> Result<XFile> {
    ensure!(data.len() >= 16, "file shorter than the 16-byte header");
    ensure!(&data[0..3] == b"xof", "not an .x file (magic {:?})", &data[0..4]);
    let version: u32 = std::str::from_utf8(&data[4..8]).ok().and_then(|s| s.trim().parse().ok()).context("bad version field")?;
    ensure!(version >= 302, "file version {version} not >= 302");
    ensure!(&data[8..11] == b"bin", "not a binary .x file");
    let mut t = Tokens { r: Reader::new(&data[16..]), count: 0, peeked: None };
    let mut file = XFile {
        header: [0; 3],
        materials: Vec::new(),
        discarded_material: None,
        meshes: Vec::new(),
        tokens: 0,
        top_level_objects: 0,
    };
    let mut seen_header = false;
    while !t.peek_is_end() {
        let o = parse_object(&mut t)?;
        let index = file.top_level_objects;
        file.top_level_objects += 1;
        match o.kind.as_str() {
            "Header" => {
                let v = ints(o.items.first(), "Header")?;
                ensure!(v.len() == 3 && o.items.len() == 1, "Header: expected three integers");
                file.header = [v[0], v[1], v[2]];
                seen_header = true;
            }
            "Material" => {
                if index == 1 {
                    file.discarded_material = Some(file.materials.len());
                }
                file.materials.push(material(&o)?);
            }
            "Mesh" => file.meshes.push(mesh(&o)?),
            other => bail!("unsupported top-level template {other} at offset {}", o.at + 16),
        }
    }
    ensure!(seen_header, "no Header object");
    file.tokens = t.count;
    Ok(file)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds binary `.x` token streams for tests.
    #[derive(Default)]
    pub struct XWriter(pub Vec<u8>);

    impl XWriter {
        pub fn new() -> Self {
            XWriter(b"xof 0302bin 0032".to_vec())
        }
        fn id(&mut self, id: u16) -> &mut Self {
            self.0.extend_from_slice(&id.to_le_bytes());
            self
        }
        pub fn name(&mut self, s: &str) -> &mut Self {
            self.id(TOK_NAME);
            self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
            self.0.extend_from_slice(s.as_bytes());
            self
        }
        pub fn string(&mut self, s: &str) -> &mut Self {
            self.id(TOK_STRING);
            self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
            self.0.extend_from_slice(s.as_bytes());
            self.id(TOK_SEMICOLON)
        }
        pub fn ints(&mut self, v: &[u32]) -> &mut Self {
            self.id(TOK_INT_LIST);
            self.0.extend_from_slice(&(v.len() as u32).to_le_bytes());
            v.iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
            self
        }
        pub fn floats(&mut self, v: &[f32]) -> &mut Self {
            self.id(TOK_FLOAT_LIST);
            self.0.extend_from_slice(&(v.len() as u32).to_le_bytes());
            v.iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
            self
        }
        pub fn open(&mut self) -> &mut Self {
            self.id(TOK_OBRACE)
        }
        pub fn close(&mut self) -> &mut Self {
            self.id(TOK_CBRACE)
        }
        pub fn material(&mut self, name: &str, texture: Option<&str>) -> &mut Self {
            self.name("Material").name(name).open().floats(&[0.5, 0.5, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
            if let Some(t) = texture {
                self.name("TextureFilename").open().string(t).close();
            }
            self.close()
        }
        /// A unit quad mesh (4 vertices, one quad face) using one referenced material.
        pub fn quad_mesh(&mut self, name: &str, material: &str) -> &mut Self {
            self.name("Mesh").name(name).open().ints(&[4]);
            self.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
            self.ints(&[1, 4, 0, 1, 2, 3]);
            self.name("MeshNormals").open().ints(&[1]).floats(&[0.0, 0.0, -1.0]).ints(&[1, 4, 0, 0, 0, 0]).close();
            self.name("MeshTextureCoords").open().ints(&[4]).floats(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]).close();
            self.name("MeshMaterialList").open().ints(&[1, 1, 0]).open().name(material).close().close();
            self.close()
        }
        pub fn header(&mut self) -> &mut Self {
            self.name("Header").open().ints(&[1, 0, 0]).close()
        }
    }

    pub fn sample() -> Vec<u8> {
        let mut w = XWriter::new();
        w.header().material("xof_default", None).material("stone", Some("wall.bmp"));
        w.quad_mesh("Wall", "stone").quad_mesh("lod_Wall", "stone").quad_mesh("lod_Missing", "stone");
        w.0
    }

    #[test]
    fn parses_synthetic_file() {
        let x = parse(&sample()).unwrap();
        assert_eq!(x.header, [1, 0, 0]);
        assert_eq!(x.materials.len(), 2);
        assert_eq!(x.discarded_material, Some(0));
        assert_eq!(x.meshes.len(), 3);
        assert_eq!(x.top_level_objects, 6);
        let m = &x.meshes[0];
        assert_eq!(m.faces, vec![vec![0, 1, 2, 3]]);
        assert_eq!(m.triangle_count(), 2);
        assert_eq!(m.fan_triangles().collect::<Vec<_>>(), vec![(0, [0, 1, 2]), (0, [0, 2, 3])]);
        assert_eq!(m.centre(), Some([0.5, 0.5, 0.0]));
        assert_eq!(x.texture_names(), vec!["wall.bmp".to_string()]);
        assert_eq!(x.lod_pairs(), vec![(1, Some(0)), (2, None)]);
        assert!(x.mesh_by_script_name("WALL").is_some());
        assert!(x.material("xof_default").is_none(), "discarded material is not referenceable");
        let s = x.stats();
        assert_eq!((s.quad_faces, s.fan_triangles, s.lod_meshes, s.lod_orphans), (3, 6, 2, 1));
        // Header 4, xof_default 5, stone 5+4, 3 meshes × 25.
        assert_eq!(x.tokens, 4 + 5 + 9 + 3 * 25);
    }

    #[test]
    fn rejects_bad_input() {
        let mut bad = sample();
        bad.extend_from_slice(&[0x0B, 0x00]);
        assert!(parse(&bad).is_err(), "stray close brace");
        let mut v = sample();
        v[4..8].copy_from_slice(b"0301");
        assert!(parse(&v).is_err(), "version < 302");
        let mut w = XWriter::new();
        w.header().name("Frame").name("f").open().close();
        assert!(parse(&w.0).is_err(), "unsupported template");
    }
}
