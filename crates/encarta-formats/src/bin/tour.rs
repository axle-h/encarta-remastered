//! Reads the 3D virtual tours (spec: docs/formats/tours-3d.md).
//!
//! tour info  <dir>                 summary counts per tour
//! tour dump  <dir> <kind|file>     JSON on stdout
//! tour check <dir>                 load everything and report cross-reference problems
//!
//! `<dir>` is one tour container (`…/WORLD/7015/04257.EIT/`) or a folder of them (`…/WORLD/7015/`;
//! `info` and `check` only). Dump kinds: summary, files, scripts, executed, deferred, macros,
//! events, model, model-stats, materials, textures, images, cursors, lights, walkmaps, layers,
//! clouds, placements, viewpoints, issues, unreferenced, inventory. Any other argument is taken as a file in
//! the container and dumped by its extension. Add `--pretty` for indented JSON.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use encarta_formats::tours::tour::{FileClass, Issue, Tour, classify, find_tours};
use encarta_formats::tours::{cloud, layer2d, lights, script, textures, viewpoints, walkmap, xfile};
use serde::Serialize;
use serde_json::json;

const USAGE: &str = "usage: tour <info|check> <dir> | tour dump <dir> <kind|file> [--pretty]";

fn emit<T: Serialize>(value: &T, pretty: bool) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    if pretty {
        serde_json::to_writer_pretty(&mut out, value)?;
    } else {
        serde_json::to_writer(&mut out, value)?;
    }
    writeln!(out)?;
    Ok(())
}

fn tours(dir: &Path) -> Result<Vec<PathBuf>> {
    let t = find_tours(dir)?;
    if t.is_empty() {
        bail!("no tour (folder with load.3sc) in {}", dir.display());
    }
    Ok(t)
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn info(dir: &Path) -> Result<()> {
    for d in tours(dir)? {
        let t = Tour::load(&d)?;
        let s = t.summary();
        println!("{}", name_of(&d));
        println!("  files {} ({})", s.files, s.files_by_class.iter().map(|(k, v)| format!("{k:?} {v}")).collect::<Vec<_>>().join(", "));
        if let Some(m) = &t.model {
            println!("  model {}: {} meshes, {} faces, {} materials, {} textures", m.file, s.meshes, s.faces, s.materials, s.textures);
        }
        if let Some(l) = &t.lights {
            println!("  lights {}: {} lights, {} list entries, {} paint records", l.file, s.lights, s.light_entries, s.paint_records);
        }
        println!("  walk maps {} ({} cells), start {}", s.walkmaps, s.walkmap_cells, t.start_walkmap.as_deref().unwrap_or("-"));
        println!("  2D layer {}: {} elements", t.final_layer().map_or("-", |l| l.file.as_str()), s.layer_elements);
        println!("  scripts {}: {} commands, {} statements, {} macros, {} RUN, {} hotspot macros", s.scripts, s.script_commands, s.script_statements, s.macro_definitions, s.run_commands, s.hotspot_macros);
        println!("  clouds {} ({} placements), view point files {}", s.clouds, s.cloud_placements, s.viewpoint_files);
        println!("  images {} ({} colour-keyed BMPs), cursors {}", s.images, s.colour_keyed_bmps, s.cursors);
        println!(
            "  issues {}: {} dangling file refs, {} silent no-ops, {} structural; {} unreferenced files",
            s.issues, s.dangling_files, s.silent_failures, s.structural_issues, s.unreferenced_files
        );
    }
    Ok(())
}

fn check(dir: &Path) -> Result<bool> {
    let mut ok = true;
    for d in tours(dir)? {
        let t = Tour::load(&d)?;
        let s = t.summary();
        println!("{}: {} files, {} issues", name_of(&d), s.files, s.issues);
        let mut silent: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for i in &t.issues {
            match i {
                Issue::UnknownMesh { id, .. } => silent.entry("unknown 3D ids").or_default().push(id.clone()),
                Issue::UnknownLight { id, .. } => silent.entry("unknown LIGHTS ids").or_default().push(id.clone()),
                Issue::UnknownElement { id, .. } => silent.entry("unknown 2D ids").or_default().push(id.clone()),
                Issue::UnknownMacro { name, .. } => silent.entry("unknown macros").or_default().push(name.clone()),
                Issue::UnknownAlarm { name, .. } => silent.entry("unknown alarms").or_default().push(name.clone()),
                Issue::UnknownSubsystem { statement, .. } => silent.entry("ignored statements").or_default().push(statement.clone()),
                other => {
                    let tag = if other.is_dangling_file() { "dangling" } else { "ERROR" };
                    if other.is_structural() {
                        ok = false;
                    }
                    println!("  {tag}: {other:?}");
                }
            }
        }
        for (k, mut v) in silent {
            let total = v.len();
            v.sort();
            v.dedup();
            println!("  silent no-ops, {k}: {total} statements, {} distinct: {}", v.len(), v.join(" "));
        }
        let unref = t.unreferenced_files();
        println!("  unreferenced files ({}): {}", unref.len(), unref.join(" "));
    }
    Ok(ok)
}

fn dump_file(t: &Tour, file: &str, pretty: bool) -> Result<()> {
    let stored = t.container.find(file).with_context(|| format!("{file} not in the container"))?;
    let bytes = t.container.read(stored)?;
    match classify(stored) {
        FileClass::Model => emit(&xfile::parse(&bytes)?, pretty),
        FileClass::Script => emit(&script::parse(&bytes), pretty),
        FileClass::WalkMap => {
            let counts = t.model.as_ref().context("walk maps need the tour's model")?.data.face_counts();
            emit(&walkmap::parse(&bytes, &counts)?, pretty)
        }
        FileClass::Lights => emit(&lights::parse(&bytes)?, pretty),
        FileClass::Layer2d => emit(&layer2d::parse(&bytes)?, pretty),
        FileClass::Cloud => emit(&cloud::parse(&bytes)?, pretty),
        FileClass::ViewPoints => emit(&viewpoints::parse(&bytes)?, pretty),
        FileClass::Image => emit(&textures::inspect(&bytes)?, pretty),
        FileClass::Cursor => emit(&textures::parse_cursor(&bytes)?, pretty),
        FileClass::Junk | FileClass::Unknown => emit(&json!({ "file": stored, "bytes": bytes.len(), "class": format!("{:?}", classify(stored)) }), pretty),
    }
}

fn dump(dir: &Path, what: &str, pretty: bool) -> Result<()> {
    let t = Tour::load(dir)?;
    match what {
        "summary" => emit(&t.summary(), pretty),
        "files" => emit(&t.container.files.iter().map(|f| json!({ "file": f, "class": format!("{:?}", classify(f)) })).collect::<Vec<_>>(), pretty),
        "scripts" => emit(&t.scripts, pretty),
        "executed" => emit(&t.executed, pretty),
        "deferred" => emit(&t.deferred, pretty),
        "macros" => emit(&t.macros, pretty),
        "events" => emit(&t.events, pretty),
        "model" => emit(&t.model, pretty),
        "model-stats" => emit(&t.model.as_ref().map(|m| m.data.stats()), pretty),
        "materials" => emit(&t.model.as_ref().map(|m| &m.data.materials), pretty),
        "textures" => emit(&t.textures, pretty),
        "images" => emit(&t.images, pretty),
        "cursors" => emit(&json!({ "slots": t.cursor_slots, "files": t.cursors }), pretty),
        "lights" => emit(&t.lights, pretty),
        "walkmaps" => emit(&t.walkmaps, pretty),
        "layers" => emit(&t.layers, pretty),
        "clouds" => emit(&t.clouds, pretty),
        "placements" => emit(&t.cloud_placements, pretty),
        "viewpoints" => emit(&t.viewpoints, pretty),
        "issues" => emit(&t.issues, pretty),
        "unreferenced" => emit(&t.unreferenced_files(), pretty),
        "inventory" => emit(&t.statement_inventory(), pretty),
        file => dump_file(&t, file, pretty),
    }
}

fn run() -> Result<ExitCode> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let pretty = args.iter().any(|a| a == "--pretty");
    args.retain(|a| a != "--pretty");
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["info", dir] => info(Path::new(dir)).map(|_| ExitCode::SUCCESS),
        ["check", dir] => check(Path::new(dir)).map(|ok| if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }),
        ["dump", dir, what] => dump(Path::new(dir), what, pretty).map(|_| ExitCode::SUCCESS),
        _ => bail!(USAGE),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tour: {e:#}");
            ExitCode::FAILURE
        }
    }
}
