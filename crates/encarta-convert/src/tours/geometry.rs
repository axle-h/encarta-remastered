//! `.x` model + `.3lt` lights + load-time object state → glTF nodes, meshes and materials.
//!
//! - Coordinates: z is negated for positions and normals, and each fan triangle's winding is
//!   reversed, `(0,i,i+1)` → `(0,i+1,i)` (§5.6). UVs are unchanged.
//! - Vertices are split on (position index, normal index, baked colour, flicker weights), since
//!   normals are indexed separately (§5.3) and light lists are per face corner (§7.2.3).
//! - Lighting is baked into `COLOR_0` with the engine's formula (§7.2.3) for every static light;
//!   flickering lights go into `_FLICKER_SLOTS[_n]` / `_FLICKER_WEIGHTS[_n]` vertex attributes
//!   (slot index and the factor `f`), so the runtime adds `trunc(current·f)` per frame.
//! - Meshes that some statement addresses, `lod_` pairs and cloud placeholders stay separate
//!   nodes (translated to their object centre); every other mesh is merged into one node per
//!   material. Within a merged primitive, triangles are ordered by (mesh, face), so every
//!   original face maps to one contiguous triangle range ([`Geometry::face_map`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};
use encarta_formats::tours::cloud::Cloud;
use encarta_formats::tours::tour::Tour;
use encarta_formats::tours::xfile::{MaterialRef, Mesh, XFile};
use serde::Serialize;
use serde_json::{Value, json};

use super::glb::{self, Builder};
use super::images;
use super::sim::LoadState;

/// LOD switch distance (horizontal XZ units, §7.6).
pub const LOD_SWITCH_DISTANCE: f64 = 13.0;

/// One glTF primitive: what the PVS ranges and the mesh map refer to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawUnit {
    pub node: String,
    pub node_index: usize,
    pub mesh: usize,
    pub primitive: usize,
    pub material: usize,
    pub triangles: u32,
    pub vertices: u32,
    pub merged: bool,
}

/// Where an original `.x` mesh ended up.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPlacement {
    pub index: usize,
    pub name: String,
    pub faces: usize,
    /// Its own node (name), or null when merged.
    pub node: Option<String>,
    /// `[drawUnit, firstTriangle, triangleCount]` per material part.
    pub parts: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialInfo {
    pub index: usize,
    pub name: String,
    /// The `.x` texture name (a "texture record"), or null for untextured materials.
    pub texture_name: Option<String>,
    /// Container file the name resolved to (§5.5), or null when missing.
    pub texture_file: Option<String>,
    pub resolution: Option<String>,
    /// glTF texture index (the texture record `APPLYTEXTURE`/`WATER` replace).
    pub gltf_texture: Option<usize>,
    pub keyed: bool,
    /// Original `.x` materials merged into this one (§5.4 identical-material merging, then by texture).
    pub source_materials: Vec<String>,
    /// Distinct `.x` face colours of those materials (not applied, §5.4).
    pub face_colors: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlickerSlot {
    pub slot: usize,
    pub light: usize,
    pub name: String,
    /// Vertices (after splitting) carrying a weight for this light.
    pub vertices: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudNode {
    pub node: String,
    pub placeholder: String,
    pub file: String,
    pub points: usize,
    /// The `.3cl` scale `s` (texels per cloud unit; 0 means 0.5).
    pub scale: f64,
    pub offset_radians: f64,
    /// Placeholder texture size (W × H texels).
    pub texture_size: [u32; 2],
    /// Placeholder quad width and height in world units.
    pub quad_size: [f32; 2],
    /// Placeholder bottom-centre point (glTF space).
    pub base: [f32; 3],
    /// Derived glTF node matrix (column-major) mapping cloud-local points to world, see docs.
    pub matrix: [f64; 16],
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStats {
    pub meshes: usize,
    pub separate_meshes: usize,
    pub merged_meshes: usize,
    /// One primitive per (mesh, material) part: what a naive export would draw.
    pub parts_before: usize,
    pub draw_units: usize,
    pub merged_nodes: usize,
    pub vertices_in: usize,
    pub vertices_out: usize,
    pub triangles: usize,
    pub normalised_normals: usize,
    pub zero_normals: usize,
    pub non_finite_uvs: usize,
    pub max_flicker_lights_per_vertex: usize,
    pub painted_corners: usize,
}

pub struct Geometry {
    pub builder: Builder,
    pub roots: Vec<usize>,
    pub draw_units: Vec<DrawUnit>,
    pub meshes: Vec<MeshPlacement>,
    /// Per mesh, per face: (draw unit, first triangle); the face covers `len − 2` triangles.
    pub face_map: Vec<Vec<(u32, u32)>>,
    pub materials: Vec<MaterialInfo>,
    pub flicker_slots: Vec<FlickerSlot>,
    pub flicker_sets: usize,
    pub clouds: Vec<CloudNode>,
    pub stats: MergeStats,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Owner {
    Mesh(usize),
    Merged,
}

/// Vertex identity within a primitive: (mesh, position index, normal index, baked colour,
/// flicker (slot, weight bits)).
type VertexKey = (u32, u32, u32, [u8; 3], Vec<(u8, u32)>);

#[derive(Default)]
struct PrimBuild {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colours: Vec<[u8; 4]>,
    flicker: Vec<Vec<(u8, f32)>>,
    indices: Vec<u32>,
    dedupe: HashMap<VertexKey, u32>,
}

/// Per-face light lists: mesh → face → (light index, vertex mask).
fn face_lights(tour: &Tour, x: &XFile) -> Vec<Vec<Vec<(u16, u32)>>> {
    let mut out: Vec<Vec<Vec<(u16, u32)>>> = x.meshes.iter().map(|m| vec![Vec::new(); m.faces.len()]).collect();
    if let Some(l) = &tour.lights {
        for (li, light) in l.data.lights.iter().enumerate() {
            for (f, mask) in light.entries() {
                if let Some(face) = out.get_mut(f.object as usize).and_then(|m| m.get_mut(f.face as usize)) {
                    face.push((li as u16, mask));
                }
            }
        }
    }
    out
}

fn material_slots(x: &XFile, m: &Mesh) -> Vec<Option<usize>> {
    // Material per face as an index into x.materials (None = default material).
    let refs: Vec<Option<usize>> = m
        .material_list
        .iter()
        .flat_map(|l| &l.materials)
        .map(|r| match r {
            MaterialRef::Named(n) => x.material(n).map(|(i, _)| i),
            MaterialRef::Inline(_) => None,
        })
        .collect();
    (0..m.faces.len()).map(|f| m.material_list.as_ref().and_then(|l| l.slot_for_face(f, m.faces.len())).and_then(|s| refs.get(s).copied().flatten())).collect()
}

fn conv(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[1], -p[2]]
}

/// Builds the glTF content. `separate` lists the mesh indices that keep their own node.
pub fn build(tour: &Tour, state: &LoadState, separate: &BTreeSet<usize>) -> Result<Geometry> {
    let x = &tour.model.as_ref().context("tour has no model")?.data;
    let mut b = Builder::default();
    b.use_extension("KHR_materials_unlit");
    b.samplers.push(json!({ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }));
    b.samplers.push(json!({ "magFilter": 9729, "minFilter": 9729, "wrapS": 10497, "wrapT": 10497 }));

    // ---- materials: one per texture name (texture record), in first-use order -------------
    let mut stats = MergeStats { meshes: x.meshes.len(), ..Default::default() };
    let slots: Vec<Vec<Option<usize>>> = x.meshes.iter().map(|m| material_slots(x, m)).collect();
    let mut mat_by_key: BTreeMap<Option<String>, usize> = BTreeMap::new();
    let mut materials: Vec<MaterialInfo> = Vec::new();
    let mut image_by_file: HashMap<String, (usize, bool)> = HashMap::new();
    let mut face_mat: Vec<Vec<usize>> = Vec::with_capacity(x.meshes.len());
    for ms in &slots {
        let mut fm = Vec::with_capacity(ms.len());
        for &s in ms {
            let src = s.map(|i| &x.materials[i]);
            let key = src.and_then(|m| m.texture.clone());
            let idx = match mat_by_key.get(&key) {
                Some(&i) => i,
                None => {
                    let tref = key.as_ref().and_then(|k| tour.textures.iter().find(|t| &t.name == k));
                    let file = tref.and_then(|t| t.file.clone());
                    let mut keyed = false;
                    let mut gltf_texture = None;
                    if let Some(f) = &file {
                        let (img, k) = match image_by_file.get(f) {
                            Some(&v) => v,
                            None => {
                                let data = tour.container.read(f)?;
                                let k = images::texture_is_keyed(&data);
                                let c = images::convert(&data, k).with_context(|| format!("texture {f}"))?;
                                let i = b.image(f, c.mime, &c.bytes);
                                image_by_file.insert(f.clone(), (i, k));
                                (i, k)
                            }
                        };
                        keyed = k;
                        b.textures.push(json!({ "name": key.clone().unwrap_or_default(), "source": img, "sampler": if k { 1 } else { 0 } }));
                        gltf_texture = Some(b.textures.len() - 1);
                    }
                    let i = materials.len();
                    materials.push(MaterialInfo {
                        index: i,
                        name: key.clone().unwrap_or_else(|| "(untextured)".into()),
                        texture_name: key.clone(),
                        texture_file: file,
                        resolution: tref.map(|t| format!("{:?}", t.resolution)),
                        gltf_texture,
                        keyed,
                        source_materials: Vec::new(),
                        face_colors: Vec::new(),
                    });
                    mat_by_key.insert(key, i);
                    i
                }
            };
            let info = &mut materials[idx];
            let src_name = src.and_then(|m| m.name.clone()).unwrap_or_else(|| "(default)".into());
            if !info.source_materials.contains(&src_name) {
                info.source_materials.push(src_name);
            }
            if let Some(m) = src
                && !info.face_colors.contains(&m.face_color)
            {
                info.face_colors.push(m.face_color);
            }
            fm.push(idx);
        }
        face_mat.push(fm);
    }
    let cloud_material = materials.len();
    for m in &materials {
        let mut v = json!({
            "name": m.name,
            "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "extensions": { "KHR_materials_unlit": {} },
            "extras": { "sourceMaterials": m.source_materials, "faceColors": m.face_colors },
        });
        if let Some(t) = m.gltf_texture {
            v["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
        }
        if m.keyed {
            v["alphaMode"] = json!("MASK");
            v["alphaCutoff"] = json!(0.5);
        }
        b.materials.push(v);
    }

    // ---- lighting inputs --------------------------------------------------------------------
    let lights_file = tour.lights.as_ref().filter(|l| state.lights_file.as_deref() == Some(l.file.as_str()));
    let lights = lights_file.map(|l| &l.data.lights[..]).unwrap_or(&[]);
    let lists = face_lights(tour, x);
    let mut flicker_slots: Vec<FlickerSlot> = Vec::new();
    let mut slot_of: HashMap<usize, u8> = HashMap::new();
    for (i, ls) in state.lights.iter().enumerate() {
        if ls.on && ls.flicker.is_some() && i < lights.len() {
            slot_of.insert(i, flicker_slots.len() as u8);
            flicker_slots.push(FlickerSlot { slot: flicker_slots.len(), light: i, name: lights[i].name.clone(), vertices: 0 });
        }
    }
    let ambient = state.ambient.map_or([0u8; 3], |a| a.map(|c| c.clamp(0, 255) as u8));
    let mut paint: HashMap<(usize, usize, usize), [u8; 3]> = HashMap::new();
    if let Some(l) = lights_file {
        for p in &l.data.paint {
            if let Some(mi) = x.meshes.iter().position(|m| m.name == p.object)
                && p.face >= 0
                && p.vertex_slot >= 0
            {
                let c = p.colour;
                paint.insert((mi, p.face as usize, p.vertex_slot as usize), [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }

    // ---- primitives ---------------------------------------------------------------------
    let mut prims: BTreeMap<(Owner, usize), (u32, PrimBuild)> = BTreeMap::new();
    let mut next_unit = 0u32;
    let mut face_map: Vec<Vec<(u32, u32)>> = Vec::with_capacity(x.meshes.len());
    let mut placements = Vec::with_capacity(x.meshes.len());
    let mut origin_of: HashMap<usize, [f32; 3]> = HashMap::new();
    let mut parts_before = 0;
    for (mi, m) in x.meshes.iter().enumerate() {
        let owner = if separate.contains(&mi) { Owner::Mesh(mi) } else { Owner::Merged };
        let origin = match owner {
            Owner::Mesh(_) => conv(m.centre().unwrap_or([0.0; 3])),
            Owner::Merged => [0.0; 3],
        };
        origin_of.insert(mi, origin);
        let obj = state.objects.get(&mi);
        let full_bright = obj.is_some_and(|o| o.sprite || o.no_illumination) || !state.lights_on || lights.is_empty();
        let mut fmap = Vec::with_capacity(m.faces.len());
        let mut parts: BTreeMap<u32, [u32; 3]> = BTreeMap::new();
        let mut distinct = BTreeSet::new();
        for (fi, face) in m.faces.iter().enumerate() {
            let mat = face_mat[mi][fi];
            distinct.insert(mat);
            let entry = prims.entry((owner, mat)).or_insert_with(|| {
                next_unit += 1;
                (next_unit - 1, PrimBuild::default())
            });
            let unit = entry.0;
            let pb = &mut entry.1;
            let first_tri = (pb.indices.len() / 3) as u32;
            fmap.push((unit, first_tri));
            let part = parts.entry(unit).or_insert([unit, first_tri, 0]);
            part[2] += face.len().saturating_sub(2) as u32;
            let mut corner_ix = Vec::with_capacity(face.len());
            for (k, &pi) in face.iter().enumerate() {
                let ni = m.normals.as_ref().and_then(|n| n.faces.get(fi)).and_then(|f| f.get(k)).copied().unwrap_or(u32::MAX);
                let pos = m.positions.get(pi as usize).copied().unwrap_or([0.0; 3]);
                let nrm = m.normals.as_ref().and_then(|n| n.normals.get(ni as usize)).copied().unwrap_or([0.0, 1.0, 0.0]);
                // Colour of this face corner (§7.2.3).
                let (colour, dynamic) = if full_bright {
                    ([255u8; 3], Vec::new())
                } else if let Some(&c) = paint.get(&(mi, fi, k)) {
                    stats.painted_corners += 1;
                    (c, Vec::new())
                } else {
                    let mut c = ambient.map(|v| v as i32);
                    let mut dynamic: Vec<(u8, u32)> = Vec::new();
                    for &(li, mask) in &lists[mi][fi] {
                        let li = li as usize;
                        let Some(ls) = state.lights.get(li) else { continue };
                        if !ls.on || k >= 32 || mask & (1 << k) == 0 {
                            continue;
                        }
                        let Some(f) = lights[li].factor(pos, nrm) else { continue };
                        if let Some(&slot) = slot_of.get(&li) {
                            dynamic.push((slot, f.to_bits()));
                        } else {
                            for (ch, base) in c.iter_mut().zip(ls.colour) {
                                *ch = (*ch + (base as f32 * f) as i32).min(255);
                            }
                        }
                    }
                    (c.map(|v| v as u8), dynamic)
                };
                let key = (mi as u32, pi, ni, colour, dynamic);
                let vi = match pb.dedupe.get(&key) {
                    Some(&v) => v,
                    None => {
                        let v = pb.positions.len() as u32;
                        let p = conv(pos);
                        pb.positions.push([p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]]);
                        let n = conv(nrm);
                        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                        let n = if len > 1e-12 {
                            if (len - 1.0).abs() > 1e-4 {
                                stats.normalised_normals += 1;
                            }
                            n.map(|c| c / len)
                        } else {
                            stats.zero_normals += 1;
                            [0.0, 1.0, 0.0]
                        };
                        pb.normals.push(n);
                        let uv = m.uvs.as_ref().and_then(|u| u.get(pi as usize)).copied().unwrap_or([0.0; 2]);
                        if !uv[0].is_finite() || !uv[1].is_finite() {
                            stats.non_finite_uvs += 1;
                        }
                        pb.uvs.push(uv.map(|c| if c.is_finite() { c } else { 0.0 }));
                        pb.colours.push([colour[0], colour[1], colour[2], 255]);
                        stats.max_flicker_lights_per_vertex = stats.max_flicker_lights_per_vertex.max(key.4.len());
                        pb.flicker.push(key.4.iter().map(|&(s, w)| (s, f32::from_bits(w))).collect());
                        pb.dedupe.insert(key, v);
                        v
                    }
                };
                corner_ix.push(vi);
            }
            for i in 1..corner_ix.len().saturating_sub(1) {
                pb.indices.extend_from_slice(&[corner_ix[0], corner_ix[i + 1], corner_ix[i]]);
            }
            stats.vertices_in += face.len();
        }
        parts_before += distinct.len();
        face_map.push(fmap);
        placements.push(MeshPlacement {
            index: mi,
            name: m.name.clone(),
            faces: m.faces.len(),
            node: matches!(owner, Owner::Mesh(_)).then(|| m.name.clone()),
            parts: parts.into_values().collect(),
        });
    }
    stats.parts_before = parts_before;
    stats.separate_meshes = separate.iter().filter(|&&i| i < x.meshes.len()).count();
    stats.merged_meshes = stats.meshes - stats.separate_meshes;

    // ---- flicker attribute sets ------------------------------------------------------------
    let flicker_sets = stats.max_flicker_lights_per_vertex.div_ceil(4);
    for (_, pb) in prims.values() {
        for v in &pb.flicker {
            for &(s, _) in v {
                flicker_slots[s as usize].vertices += 1;
            }
        }
    }

    // ---- glTF meshes and nodes ---------------------------------------------------------------
    let lod_pairs = x.lod_pairs();
    let lod_of: HashMap<usize, Option<usize>> = lod_pairs.iter().copied().collect();
    let low_detail: HashMap<usize, usize> = lod_pairs.iter().filter_map(|&(l, p)| p.map(|p| (p, l))).collect();
    let mut by_owner: BTreeMap<Owner, Vec<(usize, u32, PrimBuild)>> = BTreeMap::new();
    for ((owner, mat), (unit, pb)) in prims {
        by_owner.entry(owner).or_default().push((mat, unit, pb));
    }
    let mut draw_units: Vec<Option<DrawUnit>> = vec![None; next_unit as usize];
    let mut roots = Vec::new();
    // Separate meshes in file order (even those without faces), then merged nodes by material.
    let mut owners: Vec<Owner> = separate.iter().filter(|&&i| i < x.meshes.len()).map(|&i| Owner::Mesh(i)).collect();
    if by_owner.contains_key(&Owner::Merged) {
        owners.push(Owner::Merged);
    }
    for owner in owners {
        let parts = by_owner.remove(&owner).unwrap_or_default();
        match owner {
            Owner::Mesh(mi) => {
                let m = &x.meshes[mi];
                let mut node = json!({ "name": m.name });
                let o = origin_of[&mi];
                if o != [0.0; 3] {
                    node["translation"] = json!(o);
                }
                let mut extras = json!({ "meshIndex": mi });
                if let Some(&partner) = lod_of.get(&mi) {
                    extras["lodOf"] = json!(partner.map(|p| x.meshes[p].name.clone()));
                    extras["neverDrawnAlone"] = json!(true);
                }
                if let Some(&l) = low_detail.get(&mi) {
                    extras["lod"] = json!({ "lowDetail": x.meshes[l].name, "switchDistance": LOD_SWITCH_DISTANCE, "metric": "xz-distance-to-centre" });
                }
                node["extras"] = extras;
                let node_index = b.nodes.len();
                if !parts.is_empty() {
                    let mesh_index = b.meshes.len();
                    let prims_json: Vec<Value> = parts
                        .into_iter()
                        .enumerate()
                        .map(|(pi, (mat, unit, pb))| {
                            draw_units[unit as usize] = Some(DrawUnit {
                                node: m.name.clone(),
                                node_index,
                                mesh: mesh_index,
                                primitive: pi,
                                material: mat,
                                triangles: (pb.indices.len() / 3) as u32,
                                vertices: pb.positions.len() as u32,
                                merged: false,
                            });
                            primitive(&mut b, pb, mat, flicker_sets, &mut stats)
                        })
                        .collect();
                    b.meshes.push(json!({ "name": m.name, "primitives": prims_json }));
                    node["mesh"] = json!(mesh_index);
                }
                b.nodes.push(node);
                roots.push(node_index);
            }
            Owner::Merged => {
                for (mat, unit, pb) in parts {
                    let name = format!("merged:{}", materials[mat].name);
                    let node_index = b.nodes.len();
                    let mesh_index = b.meshes.len();
                    draw_units[unit as usize] = Some(DrawUnit {
                        node: name.clone(),
                        node_index,
                        mesh: mesh_index,
                        primitive: 0,
                        material: mat,
                        triangles: (pb.indices.len() / 3) as u32,
                        vertices: pb.positions.len() as u32,
                        merged: true,
                    });
                    let p = primitive(&mut b, pb, mat, flicker_sets, &mut stats);
                    b.meshes.push(json!({ "name": name, "primitives": [p] }));
                    b.nodes.push(json!({ "name": name, "mesh": mesh_index, "extras": { "merged": true, "material": mat } }));
                    roots.push(node_index);
                    stats.merged_nodes += 1;
                }
            }
        }
    }
    let draw_units: Vec<DrawUnit> = draw_units.into_iter().map(|d| d.expect("every unit placed")).collect();
    stats.draw_units = draw_units.len();
    anyhow::ensure!(draw_units.len() <= u16::MAX as usize, "too many draw units for 16-bit PVS runs");
    stats.triangles = draw_units.iter().map(|d| d.triangles as usize).sum();

    // ---- point clouds -------------------------------------------------------------------
    let mut clouds = Vec::new();
    let mut cloud_meshes: HashMap<String, (usize, usize)> = HashMap::new();
    for (&mi, o) in &state.objects {
        let Some((file, scale)) = &o.cloud else { continue };
        let Some(c) = tour.clouds.iter().find(|c| &c.file == file) else { continue };
        if b.materials.len() == cloud_material {
            b.materials.push(json!({ "name": "cloud points", "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 1.0 }, "extensions": { "KHR_materials_unlit": {} } }));
        }
        let (mesh_index, points) = match cloud_meshes.get(file) {
            Some(&v) => v,
            None => {
                let v = (cloud_mesh(&mut b, file, &c.data, cloud_material), c.data.points.len());
                cloud_meshes.insert(file.clone(), v);
                v
            }
        };
        let m = &x.meshes[mi];
        let tex = face_mat[mi].first().and_then(|&mat| materials[mat].texture_file.as_ref()).and_then(|f| tour.images.get(f)).map_or([0, 0], |i| [i.width, i.height]);
        let (lo, hi) = m.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
        let centre = m.centre().unwrap_or([0.0; 3]);
        let width = ((hi[0] - lo[0]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
        let height = hi[1] - lo[1];
        let s = if *scale == 0.0 { 0.5 } else { *scale };
        let off = o.cloud_offset.unwrap_or(0.0);
        let a = if tex[0] > 0 { s * width as f64 / tex[0] as f64 } else { s };
        let bb = if tex[1] > 0 { s * height as f64 / tex[1] as f64 } else { s };
        let alpha = -off - std::f64::consts::FRAC_PI_2;
        let (sn, cs) = alpha.sin_cos();
        let base = [centre[0], lo[1], -centre[2]];
        let matrix = [cs * a, 0.0, -sn * a, 0.0, 0.0, bb, 0.0, 0.0, sn * a, 0.0, cs * a, 0.0, base[0] as f64, base[1] as f64, base[2] as f64, 1.0];
        let name = format!("cloud:{}", m.name);
        let node_index = b.nodes.len();
        b.nodes.push(json!({ "name": name, "mesh": mesh_index, "matrix": matrix, "extras": { "cloudOf": m.name, "file": file } }));
        roots.push(node_index);
        clouds.push(CloudNode {
            node: name,
            placeholder: m.name.clone(),
            file: file.clone(),
            points,
            scale: *scale,
            offset_radians: off,
            texture_size: tex,
            quad_size: [width, height],
            base,
            matrix,
        });
    }

    let (lo, hi) = x.stats().bounds.unwrap_or(([0.0; 3], [0.0; 3]));
    let bmin = [lo[0], lo[1], -hi[2]];
    let bmax = [hi[0], hi[1], -lo[2]];
    let world_size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
    b.extras.insert("source".into(), json!(tour.model.as_ref().map(|m| m.file.clone())));
    b.extras.insert("worldBounds".into(), json!({ "min": bmin, "max": bmax }));
    b.extras.insert("worldSize".into(), json!(world_size));
    b.extras.insert(
        "lodPairs".into(),
        json!(lod_pairs.iter().map(|&(l, p)| json!({ "lowDetail": x.meshes[l].name, "base": p.map(|p| x.meshes[p].name.clone()) })).collect::<Vec<_>>()),
    );
    b.extras.insert(
        "textures".into(),
        json!(materials.iter().filter_map(|m| m.texture_name.as_ref().map(|n| json!({ "name": n, "file": m.texture_file, "resolution": m.resolution, "keyed": m.keyed }))).collect::<Vec<_>>()),
    );
    b.extras.insert("coordinateConversion".into(), json!("z negated, triangle winding reversed; UVs unchanged (tours-3d.md §5.6)"));

    Ok(Geometry { builder: b, roots, draw_units, meshes: placements, face_map, materials, flicker_slots, flicker_sets, clouds, stats })
}

fn primitive(b: &mut Builder, pb: PrimBuild, mat: usize, flicker_sets: usize, stats: &mut MergeStats) -> Value {
    stats.vertices_out += pb.positions.len();
    let pos = b.float_attr(&pb.positions, true);
    let nrm = b.float_attr(&pb.normals, false);
    let uv = b.float_attr(&pb.uvs, false);
    let col = b.u8x4_attr(&pb.colours, true);
    let ix = b.indices(&pb.indices);
    let mut attrs = json!({ "POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv, "COLOR_0": col });
    if pb.flicker.iter().any(|v| !v.is_empty()) {
        for set in 0..flicker_sets {
            let mut slots = Vec::with_capacity(pb.flicker.len());
            let mut weights = Vec::with_capacity(pb.flicker.len());
            for v in &pb.flicker {
                let mut s = [0u8; 4];
                let mut w = [0f32; 4];
                for (j, &(slot, f)) in v.iter().skip(4 * set).take(4).enumerate() {
                    s[j] = slot;
                    w[j] = f;
                }
                slots.push(s);
                weights.push(w);
            }
            let suffix = if set == 0 { String::new() } else { format!("_{set}") };
            attrs[format!("_FLICKER_SLOTS{suffix}")] = json!(b.u8x4_attr(&slots, false));
            attrs[format!("_FLICKER_WEIGHTS{suffix}")] = json!(b.float_attr(&weights, false));
        }
    }
    json!({ "attributes": attrs, "indices": ix, "material": mat, "mode": glb::MODE_TRIANGLES })
}

/// One POINTS mesh per cloud file: cloud-local integer coordinates (z negated), colours, and the
/// facing key (`_KEY`, degrees; −1 for NaN keys, which are always drawn).
fn cloud_mesh(b: &mut Builder, file: &str, c: &Cloud, material: usize) -> usize {
    let pos: Vec<[f32; 3]> = c.points.iter().map(|p| [p.x as f32, p.y as f32, -(p.z as f32)]).collect();
    let col: Vec<[u8; 4]> = c.points.iter().map(|p| {
        let [r, g, bl] = p.rgb();
        [r, g, bl, 255]
    }).collect();
    let key: Vec<[f32; 1]> = c.points.iter().map(|p| [if p.key.is_finite() { p.key } else { -1.0 }]).collect();
    let pa = b.float_attr(&pos, true);
    let ca = b.u8x4_attr(&col, true);
    let ka = b.float_attr(&key, false);
    b.meshes.push(json!({ "name": format!("cloud:{file}"), "primitives": [{ "attributes": { "POSITION": pa, "COLOR_0": ca, "_KEY": ka }, "material": material, "mode": glb::MODE_POINTS }] }));
    b.meshes.len() - 1
}
