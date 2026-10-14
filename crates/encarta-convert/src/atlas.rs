//! `encarta-convert atlas`: the MAD atlas (WLD) → `out/atlas/` (PMTiles rasters and vectors,
//! gazetteer, MapLibre styles, sprites, glyphs) and `out/report/atlas.json`.
//! Layout and status: docs/atlas-output.md.
//!
//! `ENCARTA_ATLAS_ONLY=raster,vector,…` limits the run to some stages (raster, vector, gazetteer,
//! style, sprites, glyphs, preview); `ENCARTA_ATLAS_RASTERS=HYPSO,MOON` limits the rasters.
//! The report is always rewritten; stages that did not run keep their previous report entry.

mod clip;
mod gazetteer;
mod geo;
mod glyphs;
mod mvt;
mod objects;
mod pixel;
mod pmtiles;
mod preview;
mod raster;
mod report;
mod shield;
mod style;
mod tables;
mod vector;

use crate::Paths;
use anyhow::{Context, Result};
use encarta_formats::mad::{Atlas, Edition};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

fn log(s: String) {
    eprintln!("[atlas] {s}");
}

fn stage_enabled(name: &str) -> bool {
    match std::env::var("ENCARTA_ATLAS_ONLY") {
        Ok(v) if !v.trim().is_empty() => v.split(',').any(|s| s.trim().eq_ignore_ascii_case(name)),
        _ => true,
    }
}

fn env_list(name: &str) -> Option<Vec<String>> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty()).map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
}

/// Loads the previous report so that a partial run keeps the entries of stages it skipped.
fn previous_report(path: &Path) -> serde_json::Map<String, Value> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            _ => e.metadata().map_or(0, |m| m.len()),
        })
        .sum()
}

pub fn run(paths: &Paths) -> Result<()> {
    let t0 = Instant::now();
    let out = paths.out.join("atlas");
    std::fs::create_dir_all(&out)?;
    let report_path = paths.out.join("report/atlas.json");
    std::fs::create_dir_all(paths.out.join("report"))?;
    let mut report = previous_report(&report_path);
    let atlas = Atlas::open_edition(&paths.extracted, Edition::World).context("opening the WLD atlas")?;
    let terrain = atlas.terrain()?;
    let styles = atlas.map_styles()?;

    if stage_enabled("raster") {
        let only = env_list("ENCARTA_ATLAS_RASTERS");
        let r = raster::run(&terrain.rasters, &styles, &out, only.as_deref(), &log)?;
        report.insert("raster".into(), serde_json::to_value(&r)?);
    }

    let classes = objects::Classes::load(&atlas)?;
    let need_objects = ["vector", "gazetteer", "glyphs"].iter().any(|s| stage_enabled(s));
    let objs = if need_objects { Some(objects::Objects::load(&atlas)?) } else { None };
    if stage_enabled("vector") {
        let t = Instant::now();
        log("vector tiles".into());
        let r = vector::run(&atlas, objs.as_ref().unwrap(), &classes, &out, &log)?;
        log(format!("vector tiles done in {:.1} s", t.elapsed().as_secs_f64()));
        report.insert("vector".into(), serde_json::to_value(&r)?);
    }

    if stage_enabled("gazetteer") {
        let t = Instant::now();
        log("gazetteer".into());
        let r = gazetteer::run(&atlas, objs.as_ref().unwrap(), &classes, &paths.extracted, &out, &log)?;
        log(format!("gazetteer done in {:.1} s", t.elapsed().as_secs_f64()));
        report.insert("gazetteer".into(), serde_json::to_value(&r)?);
    }

    let mut faces = None;
    if stage_enabled("style") {
        let detail = vector::detail_classes(&atlas, &classes)?;
        let (r, f) = style::run(&atlas, &classes, detail, &out, &log)?;
        faces = Some(f);
        report.insert("style".into(), serde_json::to_value(&r)?);
    }
    if stage_enabled("glyphs") {
        let faces = match faces {
            Some(f) => f,
            None => style::run(&atlas, &classes, vector::detail_classes(&atlas, &classes)?, &out, &log)?.1,
        };
        let mut chars: std::collections::BTreeSet<char> = std::collections::BTreeSet::new();
        for o in &objs.as_ref().unwrap().objs {
            chars.extend(o.name.raw.chars().filter(|c| !c.is_control()));
            if let Some(n) = &o.name_alt_lcid {
                chars.extend(n.chars().filter(|c| !c.is_control()));
            }
        }
        let r = glyphs::run(&paths.extracted, &faces, &chars, &out, &log)?;
        report.insert("glyphs".into(), serde_json::to_value(&r)?);
    }

    if stage_enabled("tables") {
        let t = tables::export(&atlas, &classes, &out)?;
        let m = tables::miller(&atlas, &out)?;
        log(format!("tables: mad-tables.json ({} tables), {} Miller images", t.len(), m.images.len()));
        report.insert("tables".into(), json!({ "mad_tables": t, "miller": m }));
    }

    if stage_enabled("preview") {
        let p = preview::run(&out, &paths.out.join("report"), &log)?;
        report.insert("previews".into(), json!(p));
        // Max-zoom index tiles against direct lookups in the octahedral pyramid.
        let mut checks = serde_json::Map::new();
        for r in &terrain.rasters {
            let Ok(a) = pmtiles::Reader::open(&out.join(raster::archive_name(&r.name, None))) else { continue };
            let (same, total) = raster::verify_index(r, &a, 20_000)?;
            log(format!("  {} index tiles agree with the source at {same}/{total} sampled pixels", r.name));
            checks.insert(r.name.clone(), json!({ "agree": same, "sampled": total }));
        }
        report.insert("raster_check".into(), Value::Object(checks));
    }

    let completeness = report::build(&paths.extracted, &atlas, &report)?;
    report.insert("completeness".into(), completeness);
    report.insert("output_bytes".into(), json!(dir_size(&out)));
    report.insert("generated_by".into(), json!("encarta-convert atlas"));
    report.insert("seconds_last_run".into(), json!(t0.elapsed().as_secs_f64()));
    std::fs::write(&report_path, serde_json::to_vec_pretty(&Value::Object(report))?)?;
    log(format!("done in {:.1} s", t0.elapsed().as_secs_f64()));
    Ok(())
}
