//! `encarta-convert tours`: the 7 virtual tours → `out/tours/<tourId>/`.
//!
//! Per tour: `scene.glb` (geometry, baked lighting, textures, point clouds), `scene.json` (the
//! scene description for the three.js runtime, schema in docs/tour-scene.md), `pvs.bin` (walk-map
//! visibility runs), `images/` (2D/HUD images, applied textures, the icon strip) and `cursors/`.
//! Plus `out/tours/index.json` with per-tour counts and validation results.
//!
//! Output is deterministic: running twice gives byte-identical files, files are only rewritten
//! when their content changes, and stale files inside a tour directory are removed.
//! `ENCARTA_TOURS=04257,05919` limits the run to some tours.

mod actions;
mod content;
mod geometry;
mod glb;
mod images;
mod scene;
mod sim;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use encarta_formats::tours::tour::{self, FileClass, Tour};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};

use crate::Paths;

/// One validation check.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TourReport {
    pub id: String,
    pub content_id: u64,
    pub name: String,
    pub files: BTreeMap<String, u64>,
    pub total_bytes: u64,
    pub glb: glb::GlbStats,
    pub merge: geometry::MergeStats,
    pub counts: BTreeMap<String, Value>,
    pub checks: Vec<Check>,
    #[serde(skip)]
    pub seconds: f64,
}

pub fn run(paths: &Paths) -> Result<()> {
    let root = paths.extracted.join("eit/CONTENT/WORLD/7015");
    let mut dirs = tour::find_tours(&root).with_context(|| format!("looking for tours in {}", root.display()))?;
    if let Ok(filter) = std::env::var("ENCARTA_TOURS") {
        let want: BTreeSet<String> = filter.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        dirs.retain(|d| want.contains(&tour_id(d)));
    }
    if dirs.is_empty() {
        bail!("no tours found under {}", root.display());
    }
    let out_root = paths.out.join("tours");
    std::fs::create_dir_all(&out_root)?;
    let t0 = Instant::now();
    // Tour Stops and hotspot destinations come from the catalog (`encarta-convert content`).
    let db_path = paths.out.join("content.sqlite");
    let content_ids: Vec<u64> = dirs.iter().filter_map(|d| content_id(&tour_id(d))).collect();
    let mut records = content::read_all(&db_path, &content_ids).with_context(|| format!("reading tour records from {}", db_path.display()))?;
    for site in records.values_mut().flat_map(|r| r.sites.iter_mut()) {
        if let Some(name) = &site.image_file {
            site.image_data = find_baggage(&paths.extracted, name).map(std::fs::read).transpose()?;
        }
    }
    if records.is_empty() {
        eprintln!("note: {} has no tour records; scene.json gets no stops or hotspot destinations (run `encarta-convert content` first)", db_path.display());
    }
    let reports: Vec<Result<TourReport>> = dirs
        .par_iter()
        .map(|d| {
            let id = tour_id(d);
            let rec = content_id(&id).and_then(|c| records.get(&c));
            convert_tour(d, &out_root.join(&id), rec)
        })
        .collect();
    let mut ok = Vec::new();
    let mut failed = false;
    for (d, r) in dirs.iter().zip(reports) {
        match r {
            Ok(r) => ok.push(r),
            Err(e) => {
                eprintln!("{}: {e:#}", d.display());
                failed = true;
            }
        }
    }
    println!("{:<6} {:<17} {:>10} {:>10} {:>9} {:>9} {:>6} {:>7} {:>6}  checks", "tour", "name", "glb", "json", "pvs", "images", "nodes", "units", "secs");
    for r in &ok {
        let f = |k: &str| r.files.get(k).copied().unwrap_or(0);
        let imgs: u64 = r.files.iter().filter(|(k, _)| k.starts_with("images/") || k.starts_with("cursors/")).map(|(_, v)| v).sum();
        let bad: Vec<&str> = r.checks.iter().filter(|c| !c.ok).map(|c| c.name.as_str()).collect();
        println!(
            "{:<6} {:<17} {:>10} {:>10} {:>9} {:>9} {:>6} {:>7} {:>6.1}  {}",
            r.id,
            r.name,
            f("scene.glb"),
            f("scene.json"),
            f("pvs.bin"),
            imgs,
            r.glb.nodes,
            r.merge.draw_units,
            r.seconds,
            if bad.is_empty() { format!("{} ok", r.checks.len()) } else { format!("FAILED: {}", bad.join(", ")) }
        );
        for c in r.checks.iter().filter(|c| !c.ok) {
            eprintln!("  {} {}: {}", r.id, c.name, c.detail);
        }
    }
    println!("total {:.1} s", t0.elapsed().as_secs_f64());
    write_index(&out_root, &ok)?;
    if failed || ok.iter().any(|r| r.checks.iter().any(|c| !c.ok)) {
        bail!("tour conversion finished with errors");
    }
    Ok(())
}

/// Finds a baggage file (`msencdata::baggage/<name>`) in the content packages of the extracted
/// disc (`eit/CONTENT/*/baggage/`), ignoring case.
fn find_baggage(extracted: &Path, name: &str) -> Option<std::path::PathBuf> {
    let mut packages: Vec<std::path::PathBuf> = std::fs::read_dir(extracted.join("eit/CONTENT")).ok()?.filter_map(|e| e.ok().map(|e| e.path().join("baggage"))).collect();
    packages.sort();
    packages.into_iter().find_map(|dir| {
        let exact = dir.join(name);
        if exact.is_file() {
            return Some(exact);
        }
        std::fs::read_dir(&dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path())
    })
}

/// Encarta content id of a tour: `7015` + the container number (§1).
fn content_id(id: &str) -> Option<u64> {
    format!("7015{id}").parse().ok()
}

fn tour_id(dir: &Path) -> String {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.split('.').next().unwrap_or(&name).to_string()
}

/// `index.json`: merges with entries of tours not converted in this run.
fn write_index(out_root: &Path, reports: &[TourReport]) -> Result<()> {
    let path = out_root.join("index.json");
    let mut tours: BTreeMap<String, Value> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("tours").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| Some((t.get("id")?.as_str()?.to_string(), t)))
        .collect();
    for r in reports {
        tours.insert(r.id.clone(), serde_json::to_value(r)?);
    }
    let v = json!({ "format": "encarta-tours-index", "sceneVersion": scene::SCENE_FORMAT_VERSION, "tours": tours.into_values().collect::<Vec<_>>() });
    write_if_changed(&path, &serde_json::to_vec_pretty(&v)?)
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp~");
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Writes `files` (relative paths) under `dir` and removes any other file there.
fn write_dir(dir: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for (rel, bytes) in files {
        write_if_changed(&dir.join(rel), bytes)?;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
                if !files.contains_key(&rel) {
                    std::fs::remove_file(&p)?;
                }
            }
        }
    }
    Ok(())
}

/// Everything one tour converts to, in memory.
pub struct Converted {
    pub files: BTreeMap<String, Vec<u8>>,
    pub report: TourReport,
}

fn convert_tour(dir: &Path, out: &Path, record: Option<&content::TourContent>) -> Result<TourReport> {
    let t0 = Instant::now();
    let tour = Tour::load(dir)?;
    let mut c = convert(&tour, &tour_id(dir), record)?;
    write_dir(out, &c.files)?;
    c.report.seconds = t0.elapsed().as_secs_f64();
    Ok(c.report)
}

fn check(checks: &mut Vec<Check>, name: &str, ok: bool, detail: String) {
    checks.push(Check { name: name.to_string(), ok, detail });
}

/// Converts a loaded tour into its output files and validates them. `record` is the tour's
/// content record (stops, hotspot destinations), when the catalog has been converted.
pub fn convert(tour: &Tour, id: &str, record: Option<&content::TourContent>) -> Result<Converted> {
    let x = &tour.model.as_ref().context("tour has no model")?.data;
    let requests = RefCell::new(BTreeSet::new());
    let simulation = sim::Simulation::run(tour, &requests);
    let separate = scene::addressable_meshes(tour, &simulation.sequence, &simulation.state, &requests);
    let geom = geometry::build(tour, &simulation.state, &separate)?;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // Cursors: every cursor in the container.
    let mut cursor_info = BTreeMap::new();
    for f in tour.container.of_class(FileClass::Cursor) {
        let c = images::decode_cursor(&tour.container.read(f)?).with_context(|| format!("cursor {f}"))?;
        files.insert(format!("cursors/{f}.png"), c.image.to_png()?);
        cursor_info.insert(
            f.to_string(),
            json!({ "image": format!("cursors/{f}.png"), "width": c.image.width, "height": c.image.height, "hotspot": c.hotspot, "invertedPixels": c.inverted_pixels }),
        );
    }
    let mut out = scene::build(tour, id, &simulation.sequence, &simulation.state, &geom, &requests, &cursor_info, record);
    // Site maps (§4.2): each stop's map bitmap as a PNG, with its named locations.
    let mut sites = Vec::new();
    for s in record.map_or(&[][..], |r| &r.sites[..]) {
        let (image, size) = match &s.image_data {
            Some(data) => {
                let c = images::convert(data, false).with_context(|| format!("site map {}", s.id))?;
                let path = format!("images/sitemap-{}.png", s.id);
                files.insert(path.clone(), c.bytes);
                (Some(path), [c.width, c.height])
            }
            None => (None, [0, 0]),
        };
        sites.push(json!({ "id": s.id, "file": s.image_file, "image": image, "size": size, "locations": s.locations }));
    }
    out.json["sites"] = json!(sites);
    out.json["article"] = json!(record.and_then(|r| r.article));
    out.json["tour"]["title"] = json!(record.and_then(|r| r.title.clone()));

    // Images named by actions and 2D elements, and the icon strip.
    for (stored, key) in requests.borrow().iter() {
        let path = actions::image_path(tour, stored, *key);
        if files.contains_key(&path) {
            continue;
        }
        let data = tour.container.read(stored)?;
        files.insert(path, images::convert(&data, *key).with_context(|| format!("image {stored}"))?.bytes);
    }
    if let Some((stored, Some(path))) = &simulation.state.icon_file {
        files.insert(path.clone(), images::icon_strip(&tour.container.read(stored)?)?.to_png()?);
    }
    let glb_bytes = geom.builder.to_glb(&geom.roots)?;
    let mut scene_json = out.json.clone();
    compact_floats(&mut scene_json);
    let scene_bytes = serde_json::to_vec(&scene_json)?;

    // ---- validation -------------------------------------------------------------------------
    let mut checks = Vec::new();
    let stats = match glb::check(&glb_bytes) {
        Ok(s) => {
            check(&mut checks, "glb structure", true, format!("{} nodes, {} primitives", s.nodes, s.primitives));
            s
        }
        Err(e) => {
            check(&mut checks, "glb structure", false, format!("{e:#}"));
            glb::GlbStats::default()
        }
    };
    let x_triangles: usize = x.meshes.iter().map(|m| m.triangle_count()).sum();
    check(&mut checks, "triangles = .x fan triangles", stats.triangles == x_triangles, format!("glb {} vs .x {x_triangles}", stats.triangles));
    check(&mut checks, "every .x mesh placed", geom.meshes.len() == x.meshes.len(), format!("{} vs {}", geom.meshes.len(), x.meshes.len()));
    let placed_faces: usize = geom.face_map.iter().map(Vec::len).sum();
    let x_faces: usize = x.meshes.iter().map(|m| m.faces.len()).sum();
    check(&mut checks, "every .x face mapped", placed_faces == x_faces, format!("{placed_faces} vs {x_faces}"));
    let expected_nodes = geom.stats.separate_meshes + geom.stats.merged_nodes + geom.clouds.len();
    check(&mut checks, "node count", stats.nodes == expected_nodes, format!("glb {} vs separate {} + merged {} + clouds {}", stats.nodes, geom.stats.separate_meshes, geom.stats.merged_nodes, geom.clouds.len()));
    let cloud_points: usize = {
        let mut seen = BTreeSet::new();
        geom.clouds.iter().filter(|c| seen.insert(c.file.clone())).map(|c| c.points).sum()
    };
    check(&mut checks, "cloud points", stats.points == cloud_points, format!("glb {} vs .3cl {cloud_points}", stats.points));
    check(
        &mut checks,
        "cloud placements",
        geom.clouds.len() == tour.cloud_placements.len(),
        format!("{} nodes vs {} CLOUD statements", geom.clouds.len(), tour.cloud_placements.len()),
    );
    let wm_cells: usize = tour.walkmaps.iter().map(|w| w.data.cells.len()).sum();
    check(&mut checks, "walk-map cells", out.walkmap_cells == wm_cells, format!("{} vs {wm_cells}", out.walkmap_cells));
    let mut vis_faces = 0usize;
    let mut vis_tris = 0u64;
    for w in &tour.walkmaps {
        for c in &w.data.cells {
            if let encarta_formats::tours::walkmap::CellData::Visibility(entries) = &c.data {
                for e in entries {
                    let m = &x.meshes[e.mesh as usize];
                    for f in e.visible_faces(m.faces.len()) {
                        vis_faces += 1;
                        vis_tris += m.faces[f].len().saturating_sub(2) as u64;
                    }
                }
            }
        }
    }
    check(&mut checks, "PVS faces and triangles", out.pvs_faces == vis_faces && out.pvs_triangles == vis_tris, format!("{} faces / {} triangles vs masks {vis_faces} / {vis_tris}", out.pvs_faces, out.pvs_triangles));
    check(&mut checks, "statements executed", simulation.statements == tour.executed.len(), format!("{} vs decoder {}", simulation.statements, tour.executed.len()));
    let lights_out = out.json["lights"]["lights"].as_array().map_or(0, Vec::len);
    let lights_in = tour.lights.as_ref().map_or(0, |l| l.data.lights.len());
    check(&mut checks, "lights", lights_out == lights_in, format!("{lights_out} vs {lights_in}"));
    let elements_out: usize = out.json["layers"].as_array().into_iter().flatten().filter(|l| l["final"] == true).map(|l| l["elements"].as_array().map_or(0, Vec::len)).sum();
    let elements_in = tour.final_layer().map_or(0, |l| l.data.elements.len());
    check(&mut checks, "final layer elements", elements_out == elements_in, format!("{elements_out} vs {elements_in}"));
    let summary = tour.summary();
    let hotspot_total = out.hotspot_links + out.json["unresolvedHotspots"].as_array().map_or(0, Vec::len);
    check(&mut checks, "hotspot macros", hotspot_total == summary.hotspot_macros, format!("{} links + {} on missing elements vs {} statements", out.hotspot_links, hotspot_total - out.hotspot_links, summary.hotspot_macros));
    match record {
        Some(r) => {
            let missing: Vec<String> = out.json["hotspots"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|h| h["destMedia"].is_null())
                .map(|h| h["contentIdText"].as_str().unwrap_or("?").to_string())
                .collect();
            check(
                &mut checks,
                "hotspot destinations",
                missing.is_empty(),
                if missing.is_empty() {
                    format!("{} stops, {} hotspot records, every element hotspot has a destmedia", r.stops.len(), r.hotspots.len())
                } else {
                    format!("no destmedia for {}", missing.join(", "))
                },
            );
        }
        None => check(&mut checks, "hotspot destinations", true, "no content record (content.sqlite absent): not checked".into()),
    }
    match record {
        Some(r) => {
            let mut missing: Vec<String> = r.stops.iter().filter(|s| s.site.is_none()).map(|s| format!("stop {} has no site", s.id)).collect();
            missing.extend(r.sites.iter().filter(|s| s.image_data.is_none()).map(|s| format!("site {} image {:?} not found", s.id, s.image_file)));
            check(
                &mut checks,
                "site maps",
                missing.is_empty(),
                if missing.is_empty() { format!("{} site maps for {} stops", r.sites.len(), r.stops.len()) } else { missing.join("; ") },
            );
        }
        None => check(&mut checks, "site maps", true, "no content record (content.sqlite absent): not checked".into()),
    }
    let names: BTreeSet<&str> = stats.node_names.iter().map(String::as_str).collect();
    let problems = check_names(&out.json, &names, &files, tour);
    check(&mut checks, "scene.json names exist", problems.is_empty(), if problems.is_empty() { "all node, light, element and image references resolve".into() } else { problems.join("; ") });

    let mut counts = BTreeMap::new();
    counts.insert("meshes".into(), json!(x.meshes.len()));
    counts.insert("faces".into(), json!(x_faces));
    counts.insert("triangles".into(), json!(x_triangles));
    counts.insert("separateNodes".into(), json!(geom.stats.separate_meshes));
    counts.insert("mergedNodes".into(), json!(geom.stats.merged_nodes));
    counts.insert("drawUnits".into(), json!(geom.stats.draw_units));
    counts.insert("walkmapCells".into(), json!(wm_cells));
    counts.insert("pvsRuns".into(), json!(out.pvs_runs));
    counts.insert("lights".into(), json!(lights_in));
    counts.insert("flickerLights".into(), json!(geom.flicker_slots.len()));
    counts.insert("hotspotLinks".into(), json!(out.hotspot_links));
    counts.insert("stops".into(), json!(record.map_or(0, |r| r.stops.len())));
    counts.insert("cloudPoints".into(), json!(cloud_points));
    counts.insert("loadStatements".into(), json!(simulation.statements));
    counts.insert("images".into(), json!(files.keys().filter(|k| k.starts_with("images/")).count()));
    counts.insert("cursors".into(), json!(files.keys().filter(|k| k.starts_with("cursors/")).count()));

    files.insert("scene.glb".into(), glb_bytes);
    files.insert("scene.json".into(), scene_bytes);
    files.insert("pvs.bin".into(), out.pvs);
    let sizes: BTreeMap<String, u64> = files.iter().map(|(k, v)| (k.clone(), v.len() as u64)).collect();
    let report = TourReport {
        id: id.to_string(),
        content_id: format!("7015{id}").parse().unwrap_or(0),
        name: scene::tour_name(id).to_string(),
        total_bytes: sizes.values().sum(),
        files: sizes,
        glb: stats,
        merge: geom.stats.clone(),
        counts,
        checks,
        seconds: 0.0,
    };
    Ok(Converted { files, report })
}

/// Rewrites every number that is exactly an `f32` (geometry, walk maps, light data) with its
/// shortest `f32` decimal form, which keeps the value bit-exact as `f32` and halves the text.
fn compact_floats(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64().filter(|_| n.is_f64()) {
                let g = f as f32;
                if g.is_finite() && g as f64 == f
                    && let Some(c) = format!("{g}").parse::<f64>().ok().and_then(serde_json::Number::from_f64)
                {
                    *n = c;
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(compact_floats),
        Value::Object(o) => o.values_mut().for_each(compact_floats),
        _ => {}
    }
}

const OBJECT_OPS: [&str; 6] = ["objectFlag", "applyTexture", "filmstrip", "cloud", "cloudOffset", "objectOther"];
const ELEMENT_OPS: [&str; 9] = ["elementVisible", "elementText", "elementAnchor", "elementAnchored", "elementIcon", "elementMacro", "elementNextFrame", "elementSky", "elementOther"];

/// Every `node` and 3D `target` names a glb node, every light `target` a light, every element
/// `target` an element of some layer, and every `image` a written file.
fn check_names(v: &Value, nodes: &BTreeSet<&str>, files: &BTreeMap<String, Vec<u8>>, tour: &Tour) -> Vec<String> {
    let lights: BTreeSet<&str> = tour.lights.iter().flat_map(|l| l.data.lights.iter().map(|l| l.name.as_str())).collect();
    let elements: BTreeSet<&str> = tour.layers.iter().flat_map(|l| l.data.elements.iter().map(|e| e.name.as_str())).collect();
    let mut problems = Vec::new();
    fn walk(v: &Value, f: &mut dyn FnMut(&serde_json::Map<String, Value>)) {
        match v {
            Value::Object(o) => {
                f(o);
                o.values().for_each(|c| walk(c, f));
            }
            Value::Array(a) => a.iter().for_each(|c| walk(c, f)),
            _ => {}
        }
    }
    let mut checked = 0usize;
    walk(v, &mut |o| {
        let op = o.get("op").and_then(Value::as_str).unwrap_or("");
        let target = o.get("target").and_then(Value::as_str);
        if let Some(n) = o.get("node").and_then(Value::as_str) {
            checked += 1;
            if !nodes.contains(n) {
                problems.push(format!("node {n} not in glb"));
            }
        }
        if let Some(t) = target {
            checked += 1;
            let ok = if OBJECT_OPS.contains(&op) {
                nodes.contains(t)
            } else if op == "light" {
                lights.contains(t)
            } else if ELEMENT_OPS.contains(&op) {
                elements.contains(t)
            } else {
                true
            };
            if !ok {
                problems.push(format!("{op} target {t} does not exist"));
            }
        }
        if let Some(i) = o.get("image").and_then(Value::as_str) {
            checked += 1;
            if !files.contains_key(i) {
                problems.push(format!("image {i} not written"));
            }
        }
    });
    problems.sort();
    problems.dedup();
    if checked == 0 {
        problems.push("no references found".into());
    }
    problems
}

/// Default location of the extracted disc for tests.
#[cfg(test)]
pub fn extracted_tour(id: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var_os("ENCARTA_EXTRACTED").map_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted")), std::path::PathBuf::from);
    let d = base.join(format!("eit/CONTENT/WORLD/7015/{id}.EIT"));
    d.is_dir().then_some(d)
}

#[cfg(test)]
mod tests;
