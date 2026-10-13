//! Assembles `scene.json` (schema: docs/tour-scene.md) and `pvs.bin` from the decoded tour, the
//! load simulation and the built geometry.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use encarta_formats::tours::layer2d::ElementKind;
use encarta_formats::tours::textures;
use encarta_formats::tours::tour::{FileClass, Tour, classify};
use encarta_formats::tours::walkmap::CellData;
use serde_json::{Value, json};

use super::actions::{self, Action, Ctx, ImageRequest, Step};
use super::geometry::{Geometry, LOD_SWITCH_DISTANCE};
use super::sim::{LoadState, SeqEntry};

pub const SCENE_FORMAT_VERSION: u32 = 1;

/// Bytes per run in `pvs.bin`.
pub const PVS_RUN_BYTES: usize = 8;

/// Tour names by container number (tours-3d.md §1).
pub fn tour_name(id: &str) -> &'static str {
    match id {
        "04257" => "Acropolis",
        "05285" => "Roman Forum",
        "05916" => "Beaumaris Castle",
        "05917" => "Persepolis",
        "05918" => "Colosseum",
        "05919" => "Abu Simbel",
        "05920" => "Azuchi Castle",
        _ => "",
    }
}

/// Camera defaults of the control (§6.10), used when a script leaves a property unset.
fn camera_default(prop: &str) -> Value {
    match prop {
        "speedFly" => json!(0.03),
        "speedTurn" => json!(1.3),
        "pitchSpeed" => json!(1.0),
        "height" => json!(0.05),
        "collisionRadius" => json!(0.05),
        "climbRate" => json!(0.01),
        _ => Value::Null,
    }
}

pub struct SceneOutput {
    pub json: Value,
    pub pvs: Vec<u8>,
    pub pvs_runs: usize,
    pub pvs_faces: usize,
    pub pvs_triangles: u64,
    pub hotspot_links: usize,
    pub walkmap_cells: usize,
}

fn steps(s: &[Step]) -> Value {
    json!(s)
}

fn hotspot_side(h: &encarta_formats::tours::walkmap::HotspotScript, ctx: &Ctx) -> Value {
    let mut v = json!({ "script": h.script, "steps": steps(&actions::convert_command(&h.script, ctx)) });
    if h.junk_bytes > 0 {
        v["junkBytes"] = json!(h.junk_bytes);
    }
    v
}

fn flat(polys: &[Vec<[f32; 3]>]) -> Value {
    json!(polys.iter().map(|p| p.iter().flat_map(|v| [v[0], v[1], -v[2]]).collect::<Vec<f32>>()).collect::<Vec<_>>())
}

/// Visible faces of a visibility cell → coalesced `(drawUnit, firstTriangle, count)` runs.
fn pvs_runs(entries: &[encarta_formats::tours::walkmap::VisibilityEntry], g: &Geometry, x: &encarta_formats::tours::xfile::XFile, faces: &mut usize) -> Vec<[u32; 3]> {
    let mut runs: Vec<[u32; 3]> = Vec::new();
    for e in entries {
        let mi = e.mesh as usize;
        let mesh = &x.meshes[mi];
        for f in e.visible_faces(mesh.faces.len()) {
            let (unit, first) = g.face_map[mi][f];
            let n = mesh.faces[f].len().saturating_sub(2) as u32;
            *faces += 1;
            if n > 0 {
                runs.push([unit, first, n]);
            }
        }
    }
    runs.sort_unstable();
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(runs.len());
    for r in runs {
        match out.last_mut() {
            Some(l) if l[0] == r[0] && l[1] + l[2] == r[1] => l[2] += r[2],
            _ => out.push(r),
        }
    }
    out
}

/// Builds the scene description. Images it names are added to `images`.
pub fn build(tour: &Tour, id: &str, seq: &[SeqEntry], state: &LoadState, g: &Geometry, images: &RefCell<BTreeSet<ImageRequest>>, cursor_info: &BTreeMap<String, Value>) -> SceneOutput {
    let x = &tour.model.as_ref().expect("model").data;
    let final_layer = state.current_layer().and_then(|l| tour.layers.iter().find(|t| t.file == l.file)).map(|l| &l.data);
    let ctx = Ctx { tour, layer: final_layer, images };

    // ---- camera ----------------------------------------------------------------------------
    let cam = |p: &str| state.camera.get(p).and_then(|v| v.first().copied()).map_or_else(|| camera_default(p), |v| json!(v));
    let pos = state.camera.get("pos").cloned().unwrap_or_else(|| vec![0.0, 0.2, 3.0, 0.0]);
    let camera = json!({
        "start": { "position": [pos.first(), pos.get(1), pos.get(2)], "yawDeg": pos.get(3), "pitchDeg": cam("pitch") },
        "fovDeg": cam("fov"),
        "height": cam("height"),
        "speedFly": cam("speedFly"),
        "speedTurn": cam("speedTurn"),
        "pitchSpeed": cam("pitchSpeed"),
        "pitchLimitDeg": 45.0,
        "collisionRadius": cam("collisionRadius"),
        "climbRate": cam("climbRate"),
        "near": cam("near"),
        "nearHistory": state.near_history,
        "far": cam("far"),
        "backgroundColor": state.camera.get("backgroundColor").map(|v| v.iter().map(|&c| c as i64).collect::<Vec<_>>()),
        "clearBackground": cam("clearBackground"),
        "timeFactor": { "ratePerSecond": 100.0, "min": 1.0, "max": 5.0 },
    });

    // ---- scene switches ----------------------------------------------------------------------
    let scene_val = |k: &str, d: Value| state.scene.get(k).cloned().unwrap_or(d);
    let fog_end = scene_val("fogEnd", json!(20.0));
    let scene = json!({
        "gravity": scene_val("gravity", json!(0.01)),
        "ambient": state.ambient,
        "specular": scene_val("specular", json!(false)),
        "highSpecular": scene_val("highSpecular", json!(false)),
        "displayFps": scene_val("displayFps", json!(false)),
        "halfResDriving": scene_val("halfResDriving", json!(false)),
        "fog": {
            "color": state.fog_colour.unwrap_or([0x4B, 0x96, 0xE6]),
            "start": scene_val("fogStart", json!(0.0)),
            "end": fog_end,
            "enabled": fog_end.as_f64().is_some_and(|e| e > 0.01) && state.scene.contains_key("fogEnd"),
        },
        "modelOn": state.model_on,
        "lightsOn": state.lights_on,
        "layerOn": state.current_layer().map(|l| l.on),
        "walkmapOn": state.walkmap_on,
        "timerOn": state.timer_on,
        "alarmsEnabled": state.alarms_enabled,
        "calc64bit": state.model_settings.get("calc64bit"),
        "lodDistance": state.model_settings.get("lodDistance").copied().unwrap_or(0.0),
    });

    // ---- model ------------------------------------------------------------------------------
    let lod_pairs: Vec<Value> = x
        .lod_pairs()
        .iter()
        .map(|&(l, p)| {
            let base = p.map(|p| &x.meshes[p]);
            json!({
                "lowDetail": x.meshes[l].name,
                "base": base.map(|m| m.name.clone()),
                "centre": base.and_then(|m| m.centre()).map(|c| [c[0], c[1], -c[2]]),
            })
        })
        .collect();
    let (lo, hi) = x.stats().bounds.unwrap_or(([0.0; 3], [0.0; 3]));
    let model = json!({
        "file": tour.model.as_ref().map(|m| m.file.clone()),
        "glb": "scene.glb",
        "worldBounds": { "min": [lo[0], lo[1], -hi[2]], "max": [hi[0], hi[1], -lo[2]] },
        "worldSize": (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max),
        "materials": g.materials,
        "drawUnits": g.draw_units,
        "meshes": g.meshes,
        "lod": { "switchDistance": LOD_SWITCH_DISTANCE, "metric": "horizontal (XZ) distance from the eye to the base mesh centre", "pairs": lod_pairs },
        "mergeStats": g.stats,
    });

    // ---- objects (separate nodes) -------------------------------------------------------
    let mut objects = Vec::new();
    for p in g.meshes.iter().filter(|p| p.node.is_some()) {
        let m = &x.meshes[p.index];
        let st = state.objects.get(&p.index).cloned().unwrap_or_default();
        let first_face_mat = g.face_map[p.index].first().map(|&(u, _)| g.draw_units[u as usize].material);
        let texture_record = first_face_mat.and_then(|mi| g.materials[mi].gltf_texture);
        let filmstrip_frames = st.filmstrip.and_then(|(w, _)| {
            let (stored, _) = st.applied_texture.as_ref()?;
            let info = tour.images.get(stored)?;
            (w > 0).then(|| info.width as i64 / w)
        });
        let centre = m.centre().map(|c| [c[0], c[1], -c[2]]);
        let bounds = m.bounds().map(|(a, b)| json!({ "min": [a[0], a[1], -b[2]], "max": [b[0], b[1], -a[2]] }));
        objects.push(json!({
            "node": m.name,
            "index": p.index,
            "centre": centre,
            "bounds": bounds,
            "firstFaceMaterial": first_face_mat,
            "textureRecord": texture_record,
            "state": st,
            "filmstripFrames": filmstrip_frames,
            "lodLowDetail": x.lod_pairs().iter().find(|&&(_, b)| b == Some(p.index)).map(|&(l, _)| x.meshes[l].name.clone()),
            "lodOf": m.lod_base().map(|_| x.lod_pairs().iter().find(|&&(l, _)| l == p.index).and_then(|&(_, b)| b).map(|b| x.meshes[b].name.clone())),
            "cloud": g.clouds.iter().find(|c| c.placeholder == m.name).map(|c| c.node.clone()),
        }));
    }

    // ---- lights -------------------------------------------------------------------------------
    let lights_json = match &tour.lights {
        Some(l) if state.lights_file.as_deref() == Some(l.file.as_str()) => {
            let list: Vec<Value> = l
                .data
                .lights
                .iter()
                .enumerate()
                .map(|(i, light)| {
                    let s = &state.lights[i];
                    let slot = g.flicker_slots.iter().find(|f| f.light == i);
                    json!({
                        "index": i,
                        "name": light.name,
                        "position": [light.position[0], light.position[1], -light.position[2]],
                        "range2": light.range2,
                        "colour": s.colour,
                        "fileColour": light.rgb(),
                        "on": s.on,
                        "flicker": s.flicker.map(|p| json!({ "period": p, "savedColour": s.saved, "slot": slot.map(|f| f.slot), "vertices": slot.map(|f| f.vertices) })),
                        "faceEntries": light.face_refs.len(),
                        "baked": s.on && s.flicker.is_none(),
                    })
                })
                .collect();
            json!({
                "file": l.file,
                "globalOn": state.lights_on,
                "ambient": state.ambient,
                "paintRecords": l.data.paint.len(),
                "flickerSets": g.flicker_sets,
                "flickerSlots": g.flicker_slots,
                "flickerModel": { "minFactor": 0.4, "maxFactor": 1.0, "steps": 512, "interpolation": "step" },
                "lights": list,
            })
        }
        _ => Value::Null,
    };

    // ---- walk maps + PVS --------------------------------------------------------------------
    let mut pvs: Vec<u8> = Vec::new();
    let (mut pvs_runs_total, mut pvs_faces, mut pvs_tris) = (0usize, 0usize, 0u64);
    let mut maps = Vec::new();
    let mut walkmap_cells = 0;
    let mut floors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for w in &tour.walkmaps {
        let [minx, miny, minz, maxx, maxy, maxz] = w.data.bbox;
        let mut cells = Vec::with_capacity(w.data.cells.len());
        for (ci, c) in w.data.cells.iter().enumerate() {
            let mut cell = json!({ "index": ci, "polygons": flat(&c.polygons) });
            match &c.data {
                CellData::Ground(plane) => {
                    cell["kind"] = json!("ground");
                    cell["plane"] = json!(plane.map(|[a, b, cc, d]| [a, b, -cc, d]));
                }
                CellData::Collision { level, segments, .. } => {
                    cell["kind"] = json!("collision");
                    cell["level"] = json!(level);
                    cell["segments"] = json!(segments.iter().map(|s| json!([s.x1, -s.z1, s.x2, -s.z2, s.top, s.climbable])).collect::<Vec<_>>());
                }
                CellData::Hotspot { enter, exit } => {
                    cell["kind"] = json!("hotspot");
                    let (e, xx) = (hotspot_side(enter, &ctx), hotspot_side(exit, &ctx));
                    for side in [&e, &xx] {
                        let st = side["steps"].as_array().cloned().unwrap_or_default();
                        let maps_loaded: Vec<String> = st.iter().filter(|s| s["op"] == "walkmapLoad").filter_map(|s| s["resolved"].as_str().map(str::to_string)).collect();
                        for s in st.iter().filter(|s| s["op"] == "event" && s["name"] == "USERLEVELCHANGE") {
                            if let Some(v) = s["pairs"].as_array().and_then(|p| p.iter().find(|kv| kv[0].as_str().is_some_and(|k| k.eq_ignore_ascii_case("QTSREFID")))) {
                                // The floor is the map the same string switches to, else the map the cell is in.
                                let floor = maps_loaded.last().cloned().unwrap_or_else(|| w.file.clone());
                                floors.entry(floor).or_default().insert(v[1].as_str().unwrap_or("").to_string());
                            }
                        }
                    }
                    cell["enter"] = e;
                    cell["exit"] = xx;
                }
                CellData::Visibility(entries) => {
                    cell["kind"] = json!("visibility");
                    let mut faces = 0;
                    let runs = pvs_runs(entries, g, x, &mut faces);
                    let first = pvs.len() / PVS_RUN_BYTES;
                    let mut count = 0;
                    for &[unit, start, n] in &runs {
                        pvs_tris += n as u64;
                        // 8-byte runs: u32 first triangle, u16 draw unit, u16 count (split if longer).
                        let (mut start, mut left) = (start, n);
                        while left > 0 {
                            let take = left.min(u16::MAX as u32);
                            pvs.extend_from_slice(&start.to_le_bytes());
                            pvs.extend_from_slice(&(unit as u16).to_le_bytes());
                            pvs.extend_from_slice(&(take as u16).to_le_bytes());
                            start += take;
                            left -= take;
                            count += 1;
                        }
                    }
                    let runs_len = count;
                    pvs_runs_total += runs_len;
                    pvs_faces += faces;
                    cell["pvs"] = json!({ "firstRun": first, "runCount": runs_len, "meshes": entries.len(), "faces": faces });
                }
            }
            cells.push(cell);
        }
        walkmap_cells += cells.len();
        maps.push(json!({
            "file": w.file,
            "version": w.data.version,
            "bbox": { "min": [minx, miny, -maxz], "max": [maxx, maxy, -minz] },
            "cells": cells,
        }));
    }
    let walkmaps = json!({
        "loadOrder": state.walkmaps_loaded,
        "start": state.walkmap_active,
        "pvsFile": "pvs.bin",
        "pvsRunLayout": "8 bytes little-endian: u32 firstTriangle, u16 drawUnit, u16 triangleCount",
        "grid": { "size": 4096, "gapClose": [1, 3] },
        "maps": maps,
        "floors": floors.iter().map(|(m, ids)| json!({ "walkmap": m, "qtsRefIds": ids })).collect::<Vec<_>>(),
    });

    // ---- 2D layers ----------------------------------------------------------------------------
    let mut layers = Vec::new();
    for ls in &state.layers {
        let Some(l) = tour.layers.iter().find(|t| t.file == ls.file) else { continue };
        let mut elements = Vec::new();
        for (i, e) in l.data.elements.iter().enumerate() {
            let image = if e.image.is_empty() {
                Value::Null
            } else {
                let (_, found) = textures::resolve(&e.image, |n| tour.container.find(n));
                match found {
                    Some(stored) => {
                        images.borrow_mut().insert((stored.to_string(), e.colour_key));
                        json!(actions::image_path(tour, stored, e.colour_key))
                    }
                    None => Value::Null,
                }
            };
            let st = ls.elements.iter().find(|(n, _)| n == &e.name).map(|(_, s)| json!(s));
            let kind = match e.kind {
                ElementKind::Plain => json!("plain"),
                ElementKind::Hotspot => json!("hotspot"),
                ElementKind::ButtonStrip => json!("buttonStrip"),
                ElementKind::Other(n) => json!(n),
            };
            let v = &e.vertices;
            elements.push(json!({
                "index": i,
                "name": e.name,
                "class": if e.is_hotspot_box() { "hotspot" } else if e.is_caption() { "caption" } else { "image" },
                "kind": kind,
                "rect": e.rect(),
                "uv": [v[0].u, v[0].v, v[2].u, v[2].v],
                "primitive": e.primitive,
                "additive": e.additive != 0,
                "blend": e.blend,
                "textured": e.textured,
                "capturesMouse": e.captures_mouse,
                "colorKey": e.colour_key,
                "isLabel": e.is_label,
                "hasText": e.has_text,
                "diffuse": format!("#{:06X}", e.diffuse & 0xFFFFFF),
                "textColor": format!("#{:06X}", e.text_colour & 0xFFFFFF),
                "frameColor": format!("#{:06X}", e.frame_colour & 0xFFFFFF),
                "labelFill": format!("#{:06X}", e.label_fill & 0xFFFFFF),
                "icons": { "normal": e.icon_normal, "pressed": e.icon_pressed, "hover": e.icon_hover },
                "font": { "face": e.font.face, "height": e.font.height, "weight": e.font.weight, "charset": e.font.charset },
                "fileText": e.text,
                "sourceImage": if e.image.is_empty() { Value::Null } else { json!(e.image) },
                "image": image,
                "state": st,
            }));
        }
        layers.push(json!({
            "file": ls.file,
            "version": l.data.version,
            "on": ls.on,
            "authoringIconFile": l.data.icon_file,
            "final": state.current_layer().is_some_and(|c| c.file == ls.file),
            "elements": elements,
        }));
    }

    // ---- hotspot links -------------------------------------------------------------------------
    let mut hotspots = Vec::new();
    if let Some(ls) = state.current_layer() {
        for (name, es) in &ls.elements {
            for (event, (_, st)) in &es.macros {
                for s in st {
                    if let Action::Event { name: ev, pairs, .. } = &s.action
                        && ev == "HOTSPOTCLICKED"
                        && let Some(id) = pairs.iter().find(|kv| kv[0].eq_ignore_ascii_case("HOTSPOTID"))
                    {
                        hotspots.push(json!({
                            "element": name,
                            "event": event,
                            "contentId": id[1].parse::<u64>().ok(),
                            "contentIdText": id[1],
                            "title": es.text,
                            "icon": es.icon,
                            "anchor": es.anchor,
                            "anchored": es.anchored,
                        }));
                    }
                }
            }
        }
    }
    let unresolved_hotspots: Vec<Value> = seq
        .iter()
        .filter(|e| matches!(&e.step.action, Action::ElementMacro { target: None, script, .. } if script.contains("HOTSPOTCLICKED")))
        .map(|e| json!({ "origin": e.origin, "src": e.step.src }))
        .collect();

    // ---- site map (WORLDLOADED) -----------------------------------------------------------------
    let world = state.events.iter().rev().find(|e| e.name == "WORLDLOADED");
    let get = |k: &str| world.and_then(|w| w.pairs.iter().find(|kv| kv[0].eq_ignore_ascii_case(k)).map(|kv| kv[1].clone()));
    let num = |k: &str, d: f64| get(k).map_or(d, |v| encarta_formats::tours::script::atof(&v));
    let rect_bottom = get("SM_RECTBOTTOM").or_else(|| get("SMRECTBOTTOM"));
    let site_map = json!({
        "xOffset": num("XOFFSET", 0.0),
        "zOffset": num("ZOFFSET", 0.0),
        "xExtent": num("XEXTENT", 0.0),
        "zExtent": num("ZEXTENT", 0.0),
        "mapHeight": get("MAPHEIGHT").map(|v| encarta_formats::tours::script::atof(&v)),
        "fieldOfViewDeg": num("FIELDOFVIEW", 120.0).clamp(30.0, 120.0),
        "rotateFrame": num("SM_ROTATEFRAME", 0.0),
        "rect": {
            "left": get("SM_RECTLEFT").map(|v| encarta_formats::tours::script::atoi(&v)),
            "top": get("SM_RECTTOP").map(|v| encarta_formats::tours::script::atoi(&v)),
            "right": get("SM_RECTRIGHT").map(|v| encarta_formats::tours::script::atoi(&v)),
            "bottom": rect_bottom.map(|v| encarta_formats::tours::script::atoi(&v)),
        },
        "coordinates": "original (left-handed) world: use x = x_gltf, z = -z_gltf",
        "payload": world.map(|w| w.text.clone()),
    });

    // ---- macros, alarms, events ------------------------------------------------------------
    let macros: Vec<Value> = state
        .macros
        .values()
        .map(|m| {
            json!({
                "name": m.def.name,
                "body": m.def.body,
                "terminated": m.def.terminated,
                "origin": m.origin,
                "runDuringLoad": m.run_during_load,
                "steps": steps(&actions::convert_command(&m.def.body, &ctx)),
            })
        })
        .collect();

    // ---- cursors and HUD ----------------------------------------------------------------------
    let slot_meaning = ["centre (sv)", "right", "up", "left", "down", "hotspot", "grip (2D drag)"];
    let cursor_slots: Vec<Value> = state
        .cursor_slots
        .iter()
        .map(|(slot, (file, image))| {
            json!({
                "slot": slot,
                "meaning": slot_meaning.get(*slot as usize),
                "file": file,
                "image": image,
                "hotspot": cursor_info.get(file).map(|c| c["hotspot"].clone()),
            })
        })
        .collect();
    let icon = state.icon_file.as_ref();
    let strip_info = icon.and_then(|(f, _)| tour.images.get(f));
    let count = state.icon_count.unwrap_or(0);
    let hud = json!({
        "iconStrip": icon.map(|(file, image)| json!({
            "file": file,
            "image": image,
            "width": strip_info.map(|i| i.width),
            "height": strip_info.map(|i| i.height),
            "count": count,
            "cellWidth": strip_info.map(|i| if count > 0 { i.width as i64 / count } else { 0 }),
            "cellHeight": strip_info.map(|i| i.height),
            "orientation": "file rows top-to-bottom (§8.4.1); the PNG is already in display orientation",
            "triples": ["0-2 audio", "3-5 video", "6-8 picture", "9-11 four-way arrows", "12-14 globe", "15-17 clapperboard", "18-19 empty"],
        })),
        "captions": {
            "font": state.layer_settings.get("font").cloned().unwrap_or_else(|| "Tahoma".into()),
            "fontSizePx": 13,
            "height": 22,
            "surface": [512, 32],
            "captionMetrics": { "verticalOffset": 4, "scale": 0.26, "adder": 82 },
            "widthFormula": "w = Wm + 8 - trunc((Wm - adder) * scale); +10 when colorKey (Wm measured in the XP System font)",
            "outlined": { "fill": "transparent", "text": "#FFFFFF", "outlinePx": 2, "weight": 700, "textX": 6 },
            "hotspotBox": { "fill": "icon cell top-left pixel", "text": "#C0D1E3", "frame": "#C0D1E3", "hoverText": "#FFFFFF", "hoverFrame": "#FFFFFF", "weight": 400 },
            "coneTest": "show iff dot(dirToAnchor, viewDir) > fovDeg * PI / 180 * 0.5",
        },
        "settings": state.layer_settings,
    });
    let cursors = json!({
        "enabled": state.scene.get("cursors").cloned().unwrap_or(json!(false)),
        "slots": cursor_slots,
        "files": cursor_info,
    });

    // ---- completeness ---------------------------------------------------------------------------
    let mut omitted = Vec::new();
    for f in &tour.container.files {
        let reason = match classify(f) {
            FileClass::Junk => "authoring junk (error.log, Thumbs.db, .psd)",
            FileClass::ViewPoints => ".3vp sample view points: no script loads them and the runtime has no handler (§7.7)",
            FileClass::Cursor => continue,
            _ if !tour.referenced.contains_key(f) => "referenced by nothing the engine loads (§5.7)",
            _ => continue,
        };
        omitted.push(json!({ "file": f, "reason": reason }));
    }
    let used_files: BTreeSet<&str> = g.materials.iter().filter_map(|m| m.texture_file.as_deref()).collect();
    for t in tour.textures.iter().filter(|t| !t.used) {
        if let Some(f) = t.file.as_deref().filter(|f| !used_files.contains(f) && !images.borrow().iter().any(|(s, _)| s == f)) {
            omitted.push(json!({ "file": f, "reason": format!("named only by a material no mesh uses (texture {})", t.name) }));
        }
    }

    let hotspot_links = hotspots.len();
    let json = json!({
        "format": "encarta-tour-scene",
        "version": SCENE_FORMAT_VERSION,
        "tour": { "id": id, "contentId": format!("7015{id}").parse::<u64>().ok(), "name": tour_name(id) },
        "coordinates": {
            "system": "glTF: right-handed, +Y up, metres = original model units",
            "fromOriginal": "x' = x, y' = y, z' = -z (tours-3d.md §5.6)",
            "yaw": "degrees in the original convention: look direction (sin yaw, 0, -cos yaw) in glTF space; three.js rotation.y = -yaw * PI / 180",
            "pitch": "degrees, positive looks up",
        },
        "files": { "glb": "scene.glb", "pvs": "pvs.bin" },
        "camera": camera,
        "scene": scene,
        "model": model,
        "objects": objects,
        "lights": lights_json,
        "clouds": g.clouds,
        "walkmaps": walkmaps,
        "layers": layers,
        "hud": hud,
        "cursors": cursors,
        "hotspots": hotspots,
        "unresolvedHotspots": unresolved_hotspots,
        "siteMap": site_map,
        "macros": macros,
        "alarms": state.alarms,
        "events": state.events,
        "loadSequence": seq,
        "issues": tour.issues,
        "omitted": omitted,
    });
    SceneOutput { json, pvs, pvs_runs: pvs_runs_total, pvs_faces, pvs_triangles: pvs_tris, hotspot_links, walkmap_cells }
}

/// Every mesh some statement can address (any step anywhere, run during loading or later), plus
/// `lod_` pairs and cloud placeholders: these keep their own glTF node.
pub fn addressable_meshes(tour: &Tour, seq: &[SeqEntry], state: &LoadState, images: &RefCell<BTreeSet<ImageRequest>>) -> BTreeSet<usize> {
    let x = &tour.model.as_ref().expect("model").data;
    let mut names = BTreeSet::new();
    fn visit(s: &Step, names: &mut BTreeSet<String>) {
        if s.action.is_3d_object()
            && let Some(Some(t)) = s.action.target()
        {
            names.insert(t.clone());
        }
        for n in s.action.nested() {
            visit(n, names);
        }
    }
    let final_layer = state.current_layer().and_then(|l| tour.layers.iter().find(|t| t.file == l.file)).map(|l| &l.data);
    let ctx = Ctx { tour, layer: final_layer, images };
    let mut all: Vec<Step> = seq.iter().map(|e| e.step.clone()).collect();
    for m in state.macros.values() {
        all.extend(actions::convert_command(&m.def.body, &ctx));
    }
    for a in &state.alarms {
        all.extend(a.steps.iter().cloned());
    }
    for w in &tour.walkmaps {
        for (_, enter, exit) in w.data.hotspots() {
            all.extend(actions::convert_command(&enter.script, &ctx));
            all.extend(actions::convert_command(&exit.script, &ctx));
        }
    }
    for l in &state.layers {
        for (_, e) in &l.elements {
            for (_, st) in e.macros.values() {
                all.extend(st.iter().cloned());
            }
        }
    }
    for s in &all {
        visit(s, &mut names);
    }
    let mut out: BTreeSet<usize> = x.meshes.iter().enumerate().filter(|(_, m)| names.contains(&m.name)).map(|(i, _)| i).collect();
    for (l, p) in x.lod_pairs() {
        out.insert(l);
        out.extend(p);
    }
    out.extend(state.objects.iter().filter(|(_, o)| o.cloud.is_some()).map(|(&i, _)| i));
    out
}
