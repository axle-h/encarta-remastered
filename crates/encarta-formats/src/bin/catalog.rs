//! catalog [--extracted DIR] [--kids] resolve <refId|key>   locations in every table, with package files
//! catalog [--extracted DIR] [--kids] dump                  every entry of every table, as JSON lines
//! catalog [--extracted DIR] [--kids] packages              package slots (index, GUID, file, path on disc)
//! catalog [--extracted DIR] [--kids] check                 structural checks + per-package summary
//!
//! DIR defaults to $ENCARTA_EXTRACTED, then `extracted`.

use anyhow::{Result, bail};
use encarta_formats::catalog::{
    Catalog, Edition, Table, check_package_files, find_file_ci, member_path,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::path::PathBuf;

/// True when stdout was closed early (e.g. piped into `head`).
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    use std::io::ErrorKind::BrokenPipe;
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == BrokenPipe)
            || c.downcast_ref::<serde_json::Error>()
                .and_then(|j| j.io_error_kind())
                == Some(BrokenPipe)
    })
}

fn main() {
    if let Err(e) = run() {
        if is_broken_pipe(&e) {
            return;
        }
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut extracted = std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or("extracted".into());
    let mut edition = Edition::Deluxe;
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--extracted" => extracted = args.next().map(PathBuf::from).unwrap_or_default(),
            "--kids" => edition = Edition::Kids,
            _ => rest.push(a),
        }
    }
    let paths = edition.paths(&extracted);
    let cat = Catalog::open(&paths.catalog2, &paths.ini)?;
    let mut out = BufWriter::new(std::io::stdout().lock());
    match rest
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["resolve", q] => {
            let hits = match q.parse::<u32>() {
                Ok(id) => cat.resolve_id(id),
                Err(_) => cat.resolve_key(q),
            };
            if hits.is_empty() {
                bail!("{q} not found in the catalog");
            }
            for h in hits {
                serde_json::to_writer(
                    &mut out,
                    &json!({"query": q, "table": h.table, "package": h.package,
                    "file": h.file, "local_id": h.local_id}),
                )?;
                writeln!(out)?;
            }
        }
        ["dump"] => {
            for t in Table::ALL {
                if let Some(ecn) = cat.ecn(t) {
                    for (id, l) in ecn.iter() {
                        let v = json!({"table": t, "ref_id": id, "package": l.package,
                            "file": cat.package_file(l.package), "local_id": l.local_id});
                        serde_json::to_writer(&mut out, &v)?;
                        writeln!(out)?;
                    }
                }
                if let Some(ecs) = cat.ecs(t) {
                    for (k, l) in ecs.iter() {
                        let v = json!({"table": t, "key": k, "package": l.package,
                            "file": cat.package_file(l.package), "local_id": l.local_id});
                        serde_json::to_writer(&mut out, &v)?;
                        writeln!(out)?;
                    }
                }
            }
        }
        ["packages"] => {
            for p in cat.packages.iter().flatten() {
                let path = p
                    .file
                    .as_deref()
                    .and_then(|f| find_file_ci(&paths.package_dirs, f));
                serde_json::to_writer(
                    &mut out,
                    &json!({"index": p.index, "guid": p.guid, "file": p.file,
                    "path": path}),
                )?;
                writeln!(out)?;
            }
        }
        ["check"] => {
            let mut problems = cat.validate();
            for t in Table::ALL {
                let (n, extra) = match (cat.ecn(t), cat.ecs(t)) {
                    (Some(e), _) => (e.len(), String::new()),
                    (_, Some(s)) => (s.len(), format!(" (key width {} chars)", s.width)),
                    _ => unreachable!(),
                };
                writeln!(out, "{:<12} {n:>7}{extra}", t.file_name())?;
            }
            writeln!(
                out,
                "table    pkg file               count   max_local distinct dense-in-table"
            )?;
            for u in cat.package_usage() {
                let dense =
                    u.distinct_local_ids == u.count && u.max_local_id as usize + 1 == u.count;
                writeln!(
                    out,
                    "{:<8} {:>3} {:<16} {:>8} {:>10} {:>8} {}",
                    format!("{:?}", u.table),
                    u.package,
                    u.file.as_deref().unwrap_or("?"),
                    u.count,
                    u.max_local_id,
                    u.distinct_local_ids,
                    dense
                )?;
            }
            let extents = cat.package_extents();
            let sparse: Vec<_> = extents
                .iter()
                .filter(|e| !(e.1 == e.2 as usize && e.1 == e.3))
                .collect();
            writeln!(
                out,
                "local ids per package over all tables: {} packages, {} not dense (0..n-1)",
                extents.len(),
                sparse.len()
            )?;
            for (p, n, bound, distinct) in sparse {
                problems.push(format!(
                    "package {p}: {n} entries, bound {bound}, {distinct} distinct local ids"
                ));
            }
            let files = check_package_files(&cat, &paths.package_dirs)?;
            writeln!(out, "{} packages in use, all present on disc", files.len())?;
            // Every baggage/generic key must be a member of its (extracted) EIT package.
            let iso = extracted.join("iso");
            let mut members: HashMap<u8, Option<HashSet<String>>> = HashMap::new();
            for (p, _, path) in &files {
                let unpacked = path
                    .strip_prefix(&iso)
                    .ok()
                    .map(|rel| extracted.join("eit").join(rel));
                members.insert(
                    *p,
                    unpacked.filter(|d| d.is_dir()).map(|d| list_members(&d)),
                );
            }
            let (mut found, mut missing, mut unchecked) = (0usize, 0usize, 0usize);
            for t in [Table::Baggage, Table::Generic] {
                for (k, l) in cat.ecs(t).unwrap().iter() {
                    match &members[&l.package] {
                        Some(set) if set.contains(&member_path(t, k).to_lowercase()) => found += 1,
                        Some(_) => {
                            missing += 1;
                            if missing <= 10 {
                                writeln!(
                                    out,
                                    "MISSING {t:?} {k} in {}",
                                    cat.package_file(l.package).unwrap_or("?")
                                )?;
                            }
                        }
                        None => unchecked += 1,
                    }
                }
            }
            writeln!(
                out,
                "string keys present in their EIT: {found}, missing: {missing}, not checked (AKC or not unpacked): {unchecked}"
            )?;
            if missing > 0 {
                problems.push(format!("{missing} string keys missing from their EIT"));
            }
            for p in &problems {
                writeln!(out, "PROBLEM {p}")?;
            }
            out.flush()?;
            if !problems.is_empty() {
                bail!("{} problems", problems.len());
            }
        }
        _ => bail!(
            "usage: catalog [--extracted DIR] [--kids] <resolve <id|key>|dump|packages|check>"
        ),
    }
    out.flush()?;
    Ok(())
}

/// Lower-cased relative paths of every file under an unpacked EIT directory.
fn list_members(root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.insert(rel.to_string_lossy().replace('\\', "/").to_lowercase());
            }
        }
    }
    out
}
