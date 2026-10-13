//! A small glTF 2.0 builder that writes binary `.glb` files, and a structural checker that reads
//! them back.
//!
//! The builder only covers what the tour converter needs: one binary buffer, tightly packed
//! accessors (one buffer view each), embedded images, and a JSON document assembled with
//! `serde_json`. The checker validates the container layout and every index and range a loader
//! relies on (see [`check`]).

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

pub const FLOAT: u32 = 5126;
pub const UNSIGNED_BYTE: u32 = 5121;
pub const UNSIGNED_SHORT: u32 = 5123;
pub const UNSIGNED_INT: u32 = 5125;

pub const ARRAY_BUFFER: u32 = 34962;
pub const ELEMENT_ARRAY_BUFFER: u32 = 34963;

pub const MODE_POINTS: u32 = 0;
pub const MODE_TRIANGLES: u32 = 4;

/// Accumulates the JSON arrays and the binary chunk of one glTF asset.
#[derive(Default)]
pub struct Builder {
    pub bin: Vec<u8>,
    pub buffer_views: Vec<Value>,
    pub accessors: Vec<Value>,
    pub images: Vec<Value>,
    pub samplers: Vec<Value>,
    pub textures: Vec<Value>,
    pub materials: Vec<Value>,
    pub meshes: Vec<Value>,
    pub nodes: Vec<Value>,
    pub extensions_used: Vec<String>,
    pub extras: Map<String, Value>,
}

impl Builder {
    fn align(&mut self) {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
    }

    /// Appends raw bytes as a new buffer view and returns its index.
    pub fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        self.align();
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut v = json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.buffer_views.push(v);
        self.buffer_views.len() - 1
    }

    fn accessor(&mut self, view: usize, component: u32, normalized: bool, count: usize, ty: &str, minmax: Option<(Vec<f32>, Vec<f32>)>) -> usize {
        let mut a = json!({ "bufferView": view, "componentType": component, "count": count, "type": ty });
        if normalized {
            a["normalized"] = json!(true);
        }
        if let Some((lo, hi)) = minmax {
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    /// A float vertex attribute of `N` components; `with_bounds` adds min/max (required for POSITION).
    pub fn float_attr<const N: usize>(&mut self, data: &[[f32; N]], with_bounds: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(ARRAY_BUFFER));
        let minmax = with_bounds.then(|| {
            let mut lo = [f32::INFINITY; N];
            let mut hi = [f32::NEG_INFINITY; N];
            for v in data {
                for k in 0..N {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            (lo.to_vec(), hi.to_vec())
        });
        let ty = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            _ => "VEC4",
        };
        self.accessor(view, FLOAT, false, data.len(), ty, minmax)
    }

    /// A `VEC4` attribute of unsigned bytes.
    pub fn u8x4_attr(&mut self, data: &[[u8; 4]], normalized: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flatten().copied().collect();
        let view = self.view(&bytes, Some(ARRAY_BUFFER));
        self.accessor(view, UNSIGNED_BYTE, normalized, data.len(), "VEC4", None)
    }

    /// Triangle (or point) indices; uses 16-bit indices when every index fits.
    pub fn indices(&mut self, idx: &[u32]) -> usize {
        let max = idx.iter().copied().max().unwrap_or(0);
        if max < 65535 {
            let bytes: Vec<u8> = idx.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect();
            let view = self.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
            self.accessor(view, UNSIGNED_SHORT, false, idx.len(), "SCALAR", None)
        } else {
            let bytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
            let view = self.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
            self.accessor(view, UNSIGNED_INT, false, idx.len(), "SCALAR", None)
        }
    }

    /// Embeds an image (`image/png` or `image/jpeg`) and returns its index.
    pub fn image(&mut self, name: &str, mime: &str, bytes: &[u8]) -> usize {
        let view = self.view(bytes, None);
        self.images.push(json!({ "name": name, "mimeType": mime, "bufferView": view }));
        self.images.len() - 1
    }

    pub fn use_extension(&mut self, name: &str) {
        if !self.extensions_used.iter().any(|e| e == name) {
            self.extensions_used.push(name.to_string());
        }
    }

    /// The glTF JSON document (one scene holding every root node).
    pub fn document(&self, roots: &[usize]) -> Value {
        let mut doc = json!({
            "asset": { "version": "2.0", "generator": "encarta-convert tours" },
            "scene": 0,
            "scenes": [{ "name": "tour", "nodes": roots }],
            "nodes": self.nodes,
            "meshes": self.meshes,
            "accessors": self.accessors,
            "bufferViews": self.buffer_views,
            "buffers": [{ "byteLength": self.padded_bin_len() }],
        });
        let o = doc.as_object_mut().unwrap();
        for (k, v) in [("materials", &self.materials), ("textures", &self.textures), ("images", &self.images), ("samplers", &self.samplers)] {
            if !v.is_empty() {
                o.insert(k.into(), Value::Array(v.clone()));
            }
        }
        if !self.extensions_used.is_empty() {
            o.insert("extensionsUsed".into(), json!(self.extensions_used));
        }
        if !self.extras.is_empty() {
            o.insert("extras".into(), Value::Object(self.extras.clone()));
        }
        doc
    }

    fn padded_bin_len(&self) -> usize {
        self.bin.len().next_multiple_of(4)
    }

    /// Serialises the document and binary chunk as a `.glb` file.
    pub fn to_glb(&self, roots: &[usize]) -> Result<Vec<u8>> {
        let mut json = serde_json::to_vec(&self.document(roots))?;
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let bin_len = self.padded_bin_len();
        let total = 12 + 8 + json.len() + 8 + bin_len;
        ensure!(total <= u32::MAX as usize, "glb too large");
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json);
        out.extend_from_slice(&(bin_len as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.bin);
        out.resize(total, 0);
        Ok(out)
    }
}

/// What [`check`] found in a valid file.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct GlbStats {
    pub bytes: usize,
    pub json_bytes: usize,
    pub bin_bytes: usize,
    pub nodes: usize,
    pub meshes: usize,
    pub primitives: usize,
    pub triangle_primitives: usize,
    pub point_primitives: usize,
    pub triangles: usize,
    pub points: usize,
    pub vertices: usize,
    pub materials: usize,
    pub textures: usize,
    pub images: usize,
    pub accessors: usize,
    /// Node names, in node order.
    #[serde(skip)]
    pub node_names: Vec<String>,
}

fn component_size(c: u64) -> Result<usize> {
    Ok(match c as u32 {
        5120 | UNSIGNED_BYTE => 1,
        5122 | UNSIGNED_SHORT => 2,
        UNSIGNED_INT | FLOAT => 4,
        other => bail!("bad componentType {other}"),
    })
}

fn type_components(t: &str) -> Result<usize> {
    Ok(match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        other => bail!("bad accessor type {other}"),
    })
}

fn idx(v: &Value, key: &str) -> Option<usize> {
    v.get(key).and_then(Value::as_u64).map(|n| n as usize)
}

/// Structural check of a `.glb`: header, chunk layout and alignment; every buffer view inside the
/// buffer; every accessor inside its view with a valid type, aligned and finite (floats);
/// POSITION with min/max that bound the data; index accessors in range of the vertex count; every
/// node/mesh/material/texture/image/sampler reference in range; the node tree acyclic; images
/// with a PNG or JPEG signature that matches their MIME type.
pub fn check(glb: &[u8]) -> Result<GlbStats> {
    ensure!(glb.len() >= 20, "file too short");
    ensure!(&glb[0..4] == b"glTF", "bad magic");
    let u32_at = |o: usize| u32::from_le_bytes(glb[o..o + 4].try_into().unwrap()) as usize;
    ensure!(u32_at(4) == 2, "version {} is not 2", u32_at(4));
    ensure!(u32_at(8) == glb.len(), "header length {} != file length {}", u32_at(8), glb.len());
    let json_len = u32_at(12);
    ensure!(&glb[16..20] == b"JSON", "first chunk is not JSON");
    ensure!(json_len.is_multiple_of(4), "JSON chunk not 4-byte aligned");
    let json_end = 20 + json_len;
    ensure!(json_end <= glb.len(), "JSON chunk past end");
    let doc: Value = serde_json::from_slice(&glb[20..json_end]).context("JSON chunk")?;
    let mut bin: &[u8] = &[];
    if json_end < glb.len() {
        ensure!(json_end + 8 <= glb.len(), "truncated BIN chunk header");
        let bin_len = u32_at(json_end);
        ensure!(&glb[json_end + 4..json_end + 8] == b"BIN\0", "second chunk is not BIN");
        ensure!(bin_len.is_multiple_of(4), "BIN chunk not 4-byte aligned");
        ensure!(json_end + 8 + bin_len == glb.len(), "BIN chunk length mismatch");
        bin = &glb[json_end + 8..];
    }
    ensure!(doc["asset"]["version"] == "2.0", "asset.version must be 2.0");
    let arr = |k: &str| doc.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let (buffers, views, accessors) = (arr("buffers"), arr("bufferViews"), arr("accessors"));
    let (meshes, nodes, materials, textures, images, samplers) = (arr("meshes"), arr("nodes"), arr("materials"), arr("textures"), arr("images"), arr("samplers"));
    if !buffers.is_empty() {
        ensure!(buffers.len() == 1, "only one buffer expected");
        let bl = idx(&buffers[0], "byteLength").context("buffer.byteLength")?;
        ensure!(buffers[0].get("uri").is_none(), "glb buffer 0 must not have a uri");
        ensure!(bl <= bin.len() && bin.len() - bl < 4, "buffer byteLength {bl} vs BIN {}", bin.len());
    }
    for (i, v) in views.iter().enumerate() {
        ensure!(idx(v, "buffer") == Some(0), "bufferView {i}: buffer");
        let off = idx(v, "byteOffset").unwrap_or(0);
        let len = idx(v, "byteLength").context("byteLength")?;
        ensure!(off + len <= bin.len(), "bufferView {i} past the buffer");
    }
    let mut accessor_counts = Vec::with_capacity(accessors.len());
    for (i, a) in accessors.iter().enumerate() {
        let view = idx(a, "bufferView").with_context(|| format!("accessor {i}: no bufferView"))?;
        let v = views.get(view).with_context(|| format!("accessor {i}: bufferView {view} out of range"))?;
        let comp = a["componentType"].as_u64().context("componentType")?;
        let csize = component_size(comp)?;
        let n = type_components(a["type"].as_str().context("type")?)?;
        let count = idx(a, "count").context("count")?;
        ensure!(count > 0, "accessor {i}: count 0");
        let elem = csize * n;
        let stride = idx(v, "byteStride").unwrap_or(elem);
        let a_off = idx(a, "byteOffset").unwrap_or(0);
        let v_off = idx(v, "byteOffset").unwrap_or(0);
        let v_len = idx(v, "byteLength").unwrap();
        ensure!(a_off + stride * (count - 1) + elem <= v_len, "accessor {i} past its bufferView");
        ensure!((v_off + a_off).is_multiple_of(csize), "accessor {i} misaligned");
        if comp as u32 == FLOAT {
            let base = v_off + a_off;
            let mut lo = vec![f32::INFINITY; n];
            let mut hi = vec![f32::NEG_INFINITY; n];
            for e in 0..count {
                for k in 0..n {
                    let o = base + e * stride + k * 4;
                    let f = f32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
                    ensure!(f.is_finite(), "accessor {i}: non-finite float");
                    lo[k] = lo[k].min(f);
                    hi[k] = hi[k].max(f);
                }
            }
            if let (Some(mn), Some(mx)) = (a.get("min").and_then(Value::as_array), a.get("max").and_then(Value::as_array)) {
                for k in 0..n {
                    let (mn, mx) = (mn[k].as_f64().unwrap() as f32, mx[k].as_f64().unwrap() as f32);
                    ensure!(mn == lo[k] && mx == hi[k], "accessor {i}: min/max do not match the data");
                }
            }
        }
        accessor_counts.push(count);
    }
    let read_index = |a: &Value, e: usize| -> u32 {
        let v = &views[idx(a, "bufferView").unwrap()];
        let o = idx(v, "byteOffset").unwrap_or(0) + idx(a, "byteOffset").unwrap_or(0);
        match a["componentType"].as_u64().unwrap() as u32 {
            UNSIGNED_BYTE => bin[o + e] as u32,
            UNSIGNED_SHORT => u16::from_le_bytes(bin[o + 2 * e..o + 2 * e + 2].try_into().unwrap()) as u32,
            _ => u32::from_le_bytes(bin[o + 4 * e..o + 4 * e + 4].try_into().unwrap()),
        }
    };
    let mut stats = GlbStats { bytes: glb.len(), json_bytes: json_len, bin_bytes: bin.len(), ..Default::default() };
    for (mi, m) in meshes.iter().enumerate() {
        let prims = m["primitives"].as_array().with_context(|| format!("mesh {mi}: primitives"))?;
        ensure!(!prims.is_empty(), "mesh {mi}: no primitives");
        for (pi, p) in prims.iter().enumerate() {
            let attrs = p["attributes"].as_object().context("attributes")?;
            let pos = attrs.get("POSITION").and_then(Value::as_u64).with_context(|| format!("mesh {mi}/{pi}: no POSITION"))? as usize;
            ensure!(pos < accessors.len(), "mesh {mi}/{pi}: POSITION out of range");
            ensure!(accessors[pos].get("min").is_some() && accessors[pos].get("max").is_some(), "POSITION without min/max");
            let n = accessor_counts[pos];
            for (name, a) in attrs {
                let a = a.as_u64().context("attribute index")? as usize;
                ensure!(a < accessors.len(), "mesh {mi}/{pi}: {name} out of range");
                ensure!(accessor_counts[a] == n, "mesh {mi}/{pi}: {name} count {} != POSITION count {n}", accessor_counts[a]);
                let comp = accessors[a]["componentType"].as_u64().unwrap() as u32;
                let elem = component_size(comp as u64)? * type_components(accessors[a]["type"].as_str().unwrap())?;
                ensure!(elem.is_multiple_of(4), "mesh {mi}/{pi}: {name} element size {elem} not a multiple of 4");
                if !name.starts_with('_') {
                    let ok = match name.as_str() {
                        "POSITION" | "NORMAL" => comp == FLOAT && accessors[a]["type"] == "VEC3",
                        "TEXCOORD_0" => comp == FLOAT && accessors[a]["type"] == "VEC2",
                        "COLOR_0" => accessors[a]["type"] == "VEC3" || accessors[a]["type"] == "VEC4",
                        _ => false,
                    };
                    ensure!(ok, "mesh {mi}/{pi}: unexpected attribute {name} layout");
                }
            }
            stats.vertices += n;
            let mode = p.get("mode").and_then(Value::as_u64).unwrap_or(4) as u32;
            if let Some(ix) = idx(p, "indices") {
                ensure!(ix < accessors.len(), "mesh {mi}/{pi}: indices out of range");
                let a = &accessors[ix];
                ensure!(a["type"] == "SCALAR" && a["componentType"] != FLOAT, "indices must be unsigned integer scalars");
                let count = accessor_counts[ix];
                for e in 0..count {
                    ensure!((read_index(a, e) as usize) < n, "mesh {mi}/{pi}: index out of range");
                }
                match mode {
                    MODE_TRIANGLES => {
                        ensure!(count.is_multiple_of(3), "mesh {mi}/{pi}: index count not a multiple of 3");
                        stats.triangles += count / 3;
                    }
                    MODE_POINTS => stats.points += count,
                    other => bail!("unexpected mode {other}"),
                }
            } else {
                match mode {
                    MODE_TRIANGLES => stats.triangles += n / 3,
                    MODE_POINTS => stats.points += n,
                    other => bail!("unexpected mode {other}"),
                }
            }
            match mode {
                MODE_TRIANGLES => stats.triangle_primitives += 1,
                _ => stats.point_primitives += 1,
            }
            if let Some(mat) = idx(p, "material") {
                ensure!(mat < materials.len(), "mesh {mi}/{pi}: material out of range");
            }
            stats.primitives += 1;
        }
    }
    for (i, m) in materials.iter().enumerate() {
        if let Some(t) = m.pointer("/pbrMetallicRoughness/baseColorTexture/index").and_then(Value::as_u64) {
            ensure!((t as usize) < textures.len(), "material {i}: texture out of range");
        }
    }
    for (i, t) in textures.iter().enumerate() {
        let s = idx(t, "source").with_context(|| format!("texture {i}: no source"))?;
        ensure!(s < images.len(), "texture {i}: image out of range");
        if let Some(s) = idx(t, "sampler") {
            ensure!(s < samplers.len(), "texture {i}: sampler out of range");
        }
    }
    for (i, im) in images.iter().enumerate() {
        let v = idx(im, "bufferView").with_context(|| format!("image {i}: no bufferView"))?;
        let view = views.get(v).context("image bufferView out of range")?;
        let o = idx(view, "byteOffset").unwrap_or(0);
        let data = &bin[o..o + idx(view, "byteLength").unwrap()];
        match im["mimeType"].as_str() {
            Some("image/png") => ensure!(data.starts_with(b"\x89PNG\r\n\x1a\n"), "image {i}: not a PNG"),
            Some("image/jpeg") => ensure!(data.starts_with(&[0xFF, 0xD8]), "image {i}: not a JPEG"),
            other => bail!("image {i}: unsupported mimeType {other:?}"),
        }
    }
    let mut parent = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let Some(m) = idx(n, "mesh") {
            ensure!(m < meshes.len(), "node {i}: mesh out of range");
        }
        for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
            let c = c.as_u64().context("child")? as usize;
            ensure!(c < nodes.len() && c != i, "node {i}: bad child {c}");
            ensure!(parent[c].replace(i).is_none(), "node {c} has two parents");
        }
        if let Some(m) = n.get("matrix").and_then(Value::as_array) {
            ensure!(m.len() == 16, "node {i}: matrix length");
            ensure!(n.get("translation").is_none() && n.get("rotation").is_none() && n.get("scale").is_none(), "node {i}: matrix and TRS");
        }
        stats.node_names.push(n.get("name").and_then(Value::as_str).unwrap_or("").to_string());
    }
    for start in 0..nodes.len() {
        let (mut at, mut steps) = (start, 0);
        while let Some(p) = parent[at] {
            at = p;
            steps += 1;
            ensure!(steps <= nodes.len(), "node hierarchy has a cycle");
        }
    }
    for s in doc.get("scenes").and_then(Value::as_array).into_iter().flatten() {
        for n in s.get("nodes").and_then(Value::as_array).into_iter().flatten() {
            let n = n.as_u64().context("scene node")? as usize;
            ensure!(n < nodes.len() && parent[n].is_none(), "scene root {n} invalid");
        }
    }
    stats.nodes = nodes.len();
    stats.meshes = meshes.len();
    stats.materials = materials.len();
    stats.textures = textures.len();
    stats.images = images.len();
    stats.accessors = accessors.len();
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_asset(indices: &[u32]) -> Builder {
        let mut b = Builder::default();
        let pos = b.float_attr(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], true);
        let col = b.u8x4_attr(&[[255, 0, 0, 255]; 3], true);
        let ix = b.indices(indices);
        b.meshes.push(json!({ "primitives": [{ "attributes": { "POSITION": pos, "COLOR_0": col }, "indices": ix }] }));
        b.nodes.push(json!({ "name": "tri", "mesh": 0 }));
        b
    }

    #[test]
    fn round_trip_and_check() {
        let glb = triangle_asset(&[0, 1, 2]).to_glb(&[0]).unwrap();
        assert_eq!(glb.len() % 4, 0);
        let s = check(&glb).unwrap();
        assert_eq!((s.nodes, s.primitives, s.triangles, s.vertices), (1, 1, 1, 3));
        assert_eq!(s.node_names, ["tri"]);
    }

    #[test]
    fn check_rejects_out_of_range_indices() {
        let glb = triangle_asset(&[0, 1, 3]).to_glb(&[0]).unwrap();
        assert!(check(&glb).unwrap_err().to_string().contains("index out of range"));
    }

    #[test]
    fn check_rejects_bad_minmax_and_truncation() {
        let mut b = triangle_asset(&[0, 1, 2]);
        b.accessors[0]["max"] = json!([5.0, 1.0, 0.0]);
        assert!(check(&b.to_glb(&[0]).unwrap()).is_err());
        let glb = triangle_asset(&[0, 1, 2]).to_glb(&[0]).unwrap();
        assert!(check(&glb[..glb.len() - 4]).is_err());
    }

    #[test]
    fn wide_indices_switch_to_u32() {
        let mut b = Builder::default();
        let a = b.indices(&[0, 70000]);
        assert_eq!(b.accessors[a]["componentType"], UNSIGNED_INT);
        let a = b.indices(&[0, 1000]);
        assert_eq!(b.accessors[a]["componentType"], UNSIGNED_SHORT);
    }
}
