//! `encarta-convert media`: every media file on the disc → content-addressed files under
//! `out/media/`, `out/media/manifest.sqlite`, and the completeness report
//! `out/report/media.json` (+ `media-members.tsv`). See docs/media.md.
//!
//! Stages: survey (walk `extracted/eit` and `extracted/iso`, sniff, classify, hash) → catalog
//! keys and `.iax` references → plan (skip outputs already present with the right hash) →
//! transcode on a bounded worker pool → report. Re-running resumes where a previous run stopped.
//!
//! Environment knobs (main.rs stays generic): `MEDIA_JOBS` (worker count, default 8),
//! `MEDIA_ONLY` (comma-separated substrings of `container/member`; restricts the run to a subset,
//! for tests; use a separate `--out`).

pub mod atlas;
pub(crate) mod ffmpeg;
mod iax;
mod jobs;
mod manifest;
mod rules;
mod sami;
mod sniff;
mod still;
mod store;

use crate::Paths;
use anyhow::{Context, Result};
use encarta_formats::catalog::{self, Catalog, Edition, Table};
use jobs::{Source, Spec};
use manifest::DerivRow;
use rayon::prelude::*;
use rules::{Disposition, Kind};
use serde_json::json;
use sniff::Format;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use store::Store;

/// One file on the disc: an EIT member, or a loose ISO file (container `ISO`).
#[derive(Debug, Clone)]
struct Member {
    container: String,
    member: String,
    abs: PathBuf,
    size: u64,
    fmt: Option<Format>,
    disp: Disposition,
    sha: Option<String>,
}

impl Member {
    fn package(&self) -> &str {
        self.container.rsplit('/').next().unwrap_or(&self.container)
    }
    fn key(&self) -> String {
        format!("{}/{}", self.container, self.member)
    }
}

const CONTAINER_EXTS: &[&str] = &[".EIT", ".ITR", ".DLX", ".KDC"];

fn is_container_name(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    CONTAINER_EXTS.iter().any(|e| u.ends_with(e))
}

fn sniff_file(path: &Path) -> Format {
    let mut buf = [0u8; sniff::SNIFF_LEN];
    let n = std::fs::File::open(path).and_then(|mut f| {
        let mut total = 0;
        loop {
            let k = f.read(&mut buf[total..])?;
            if k == 0 || total + k == buf.len() {
                return Ok(total + k);
            }
            total += k;
        }
    });
    sniff::sniff(&buf[..n.unwrap_or(0)])
}

fn filter_from_env() -> Vec<String> {
    std::env::var("MEDIA_ONLY")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn rel_string(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Walks the unpacked containers and the ISO and classifies every file.
fn survey(extracted: &Path, filter: &[String]) -> Result<Vec<Member>> {
    let keep = |key: &str| filter.is_empty() || filter.iter().any(|f| key.to_ascii_lowercase().contains(f));
    let mut raw = Vec::new();
    let eit = extracted.join("eit");
    for entry in walkdir::WalkDir::new(&eit).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = rel_string(entry.path().strip_prefix(&eit)?);
        let parts: Vec<&str> = rel.split('/').collect();
        let (container, member) = match parts.iter().position(|p| is_container_name(p)) {
            Some(i) if i + 1 < parts.len() => (parts[..=i].join("/"), parts[i + 1..].join("/")),
            _ => ("(no container)".to_string(), rel.clone()),
        };
        if keep(&format!("{container}/{member}")) {
            raw.push((container, member, entry.path().to_path_buf(), entry.metadata()?.len(), false));
        }
    }
    let iso = extracted.join("iso");
    for entry in walkdir::WalkDir::new(&iso).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = rel_string(entry.path().strip_prefix(&iso)?);
        if keep(&format!("ISO/{rel}")) {
            raw.push(("ISO".to_string(), rel, entry.path().to_path_buf(), entry.metadata()?.len(), true));
        }
    }
    let members: Vec<Member> = raw
        .into_par_iter()
        .map(|(container, member, abs, size, loose)| {
            // Dictionary EITs hold ~290k XML entries owned by the content converter: no need to read them.
            let skip_sniff = container.to_ascii_uppercase().starts_with("EE/ENCARTA/EDICT/");
            let fmt = (!skip_sniff).then(|| sniff_file(&abs));
            let f = fmt.unwrap_or(Format::Other);
            let mut disp = if loose {
                rules::classify_iso(&member, f)
            } else if container == "(no container)" {
                Disposition::Unexplained
            } else {
                rules::classify_member(&container, &member, f)
            };
            if loose && disp == Disposition::Container && !eit.join(&member).is_dir() {
                disp = Disposition::Unexplained;
            }
            Member { container, member, abs, size, fmt, disp, sha: None }
        })
        .collect();
    // Hash every media file.
    let members = members
        .into_par_iter()
        .map(|mut m| -> Result<Member> {
            if matches!(m.disp, Disposition::Media { .. }) {
                m.sha = Some(store::sha256_file(&m.abs)?);
            }
            Ok(m)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(members)
}

/// Groups media members by content into sources.
fn sources(members: &[Member]) -> (Vec<Source>, HashMap<String, Vec<usize>>, usize) {
    let mut by_sha: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, m) in members.iter().enumerate() {
        if let Some(sha) = &m.sha {
            by_sha.entry(sha.clone()).or_default().push(i);
        }
    }
    let mut conflicts = 0;
    let mut out = Vec::new();
    for (sha, idxs) in &by_sha {
        let mut kinds: Vec<(Kind, Option<rules::ColourKey>)> = idxs
            .iter()
            .filter_map(|&i| match members[i].disp {
                Disposition::Media { kind, key } => Some((kind, key)),
                _ => None,
            })
            .collect();
        kinds.sort();
        kinds.dedup();
        if kinds.iter().map(|k| k.0).collect::<std::collections::BTreeSet<_>>().len() > 1 {
            conflicts += 1;
        }
        let kind = kinds[0].0;
        let key = kinds.iter().filter_map(|k| k.1).max();
        let first = &members[idxs[0]];
        out.push(Source {
            sha: sha.clone(),
            path: first.abs.clone(),
            size: first.size,
            fmt: first.fmt.unwrap_or(Format::Other),
            kind,
            key,
            meta: None,
        });
    }
    (out, by_sha.into_iter().collect(), conflicts)
}

struct IaxRef {
    iax_sha: String,
    element: String,
    name: String,
    resolution: &'static str,
    target: Option<usize>,
}

fn edition_of(container: &str) -> &'static str {
    if container.to_ascii_uppercase().starts_with("EE/KIDS/") { "kids" } else { "deluxe" }
}

/// Resolves the file names an `.iax` mentions to members (docs/media.md §2.7).
fn resolve_iax(
    members: &[Member],
    iax_idx: usize,
    name: &str,
    by_path: &HashMap<(String, String), usize>,
    by_base: &HashMap<String, Vec<usize>>,
) -> (&'static str, Option<usize>) {
    let m = &members[iax_idx];
    let name_lc = name.to_ascii_lowercase().replace('\\', "/");
    let member_lc = m.member.to_ascii_lowercase();
    let dir = member_lc.rsplit_once('/').map_or("", |(d, _)| d);
    let joined = if dir.is_empty() { name_lc.clone() } else { format!("{dir}/{name_lc}") };
    if let Some(&i) = by_path.get(&(m.container.clone(), joined)) {
        return ("relative", Some(i));
    }
    let base = name_lc.rsplit('/').next().unwrap_or(&name_lc);
    let Some(cands) = by_base.get(base) else { return ("unresolved", None) };
    // Baggage `.iax` files keep their assets in `dswMedia/IAF/<lang>/<iax id>/`.
    let stem = member_lc.rsplit('/').next().unwrap_or("").trim_end_matches(".iax");
    let pick = |filter: &dyn Fn(&Member) -> bool| -> Option<(bool, usize)> {
        let c: Vec<usize> = cands.iter().copied().filter(|&i| filter(&members[i])).collect();
        let first = *c.first()?;
        let same = c.iter().all(|&i| members[i].sha == members[first].sha);
        Some((same, first))
    };
    let iaf_dir = format!("/{stem}/");
    type Test<'a> = &'a dyn Fn(&Member) -> bool;
    let tries: [(&'static str, Test); 4] = [
        ("iaf-folder", &|c: &Member| c.container == m.container && c.member.to_ascii_lowercase().contains(&iaf_dir)),
        ("same-container", &|c: &Member| c.container == m.container),
        ("same-edition", &|c: &Member| edition_of(&c.container) == edition_of(&m.container)),
        ("any-container", &|_: &Member| true),
    ];
    for (label, f) in tries {
        if let Some((unique, i)) = pick(f) {
            return (if unique { label } else { "ambiguous" }, Some(i));
        }
    }
    ("unresolved", None)
}

struct CatalogRow {
    edition: &'static str,
    table: &'static str,
    key: String,
    package_index: u8,
    package_file: Option<String>,
    local_id: u32,
    target: Option<(String, String)>,
    sha: Option<String>,
}

fn catalog_rows(
    extracted: &Path,
    members: &[Member],
    by_path: &HashMap<(String, String), usize>,
) -> Result<(Vec<CatalogRow>, serde_json::Value)> {
    let iso = extracted.join("iso");
    let eit = extracted.join("eit");
    let mut rows = Vec::new();
    let mut summary = serde_json::Map::new();
    for (edition, name) in [(Edition::Deluxe, "deluxe"), (Edition::Kids, "kids")] {
        let p = edition.paths(extracted);
        if !p.catalog2.is_dir() || !p.ini.is_file() {
            summary.insert(name.into(), json!({ "error": "catalog not found" }));
            continue;
        }
        let cat = Catalog::open(&p.catalog2, &p.ini).with_context(|| format!("{name} catalog"))?;
        // Package index → container path under extracted/eit (matched ignoring case).
        let dirs: Vec<PathBuf> = p.package_dirs.iter().filter_map(|d| d.strip_prefix(&iso).ok()).map(|r| eit.join(r)).collect();
        let mut containers: HashMap<u8, Option<String>> = HashMap::new();
        let (mut total, mut in_eit, mut media, mut missing) = (0, 0, 0, 0);
        for (table, ecs, tname) in [(Table::Baggage, &cat.baggage, "baggage"), (Table::Generic, &cat.generic, "generic")] {
            for (key, loc) in ecs.iter() {
                total += 1;
                let container = containers
                    .entry(loc.package)
                    .or_insert_with(|| {
                        let file = cat.package_file(loc.package)?;
                        let found = catalog::find_file_ci(&dirs, file).filter(|f| f.is_dir())?;
                        Some(rel_string(found.strip_prefix(&eit).ok()?))
                    })
                    .clone();
                let member_path = catalog::member_path(table, key);
                let hit = container
                    .as_ref()
                    .and_then(|c| by_path.get(&(c.clone(), member_path.to_ascii_lowercase())))
                    .map(|&i| &members[i]);
                if container.is_some() {
                    in_eit += 1;
                    if hit.is_none() {
                        missing += 1;
                    }
                }
                if hit.is_some_and(|m| m.sha.is_some()) {
                    media += 1;
                }
                rows.push(CatalogRow {
                    edition: name,
                    table: tname,
                    key: key.to_string(),
                    package_index: loc.package,
                    package_file: cat.package_file(loc.package).map(str::to_string),
                    local_id: loc.local_id,
                    target: hit.map(|m| (m.container.clone(), m.member.clone())),
                    sha: hit.and_then(|m| m.sha.clone()),
                });
            }
        }
        summary.insert(
            name.into(),
            json!({ "keys": total, "keys_in_eit_packages": in_eit, "resolved_to_media_files": media, "eit_keys_without_member": missing }),
        );
    }
    Ok((rows, serde_json::Value::Object(summary)))
}

fn disp_fields(d: &Disposition) -> (&'static str, Option<&'static str>, Option<String>) {
    match d {
        Disposition::Media { kind, .. } => (if kind.converts() { "converted" } else { "kept" }, Some("media"), None),
        Disposition::Delegated { owner, reason } => ("delegated", Some(owner), Some(reason.to_string())),
        Disposition::Omitted { reason } => ("omitted", None, Some(reason.to_string())),
        Disposition::Container => ("container", None, Some("ITOLITLS container; its members are classified one by one".into())),
        Disposition::Unexplained => ("unexplained", None, Some("no rule matches this file".into())),
    }
}

/// Final status of a member, given the failures of its source.
fn member_status(m: &Member, failures: &HashMap<String, Vec<(String, String)>>) -> (String, Option<&'static str>, Option<String>) {
    let (status, owner, reason) = disp_fields(&m.disp);
    if let Some(f) = m.sha.as_ref().and_then(|s| failures.get(s)) {
        let real: Vec<&(String, String)> = f.iter().filter(|(_, e)| !e.starts_with("not applicable")).collect();
        if !real.is_empty() {
            let why = real.iter().map(|(fmt, e)| format!("{fmt}: {e}")).collect::<Vec<_>>().join("; ");
            return ("failed".into(), owner, Some(why));
        }
    }
    (status.into(), owner, reason)
}

fn kind_of(m: &Member) -> Option<Kind> {
    match m.disp {
        Disposition::Media { kind, .. } => Some(kind),
        _ => None,
    }
}

enum Msg {
    Done { sha: String, outcome: jobs::Outcome },
}

pub fn run(paths: &Paths) -> Result<()> {
    let t0 = Instant::now();
    let filter = filter_from_env();
    let workers: usize = std::env::var("MEDIA_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8).max(1);
    let media_dir = paths.out.join("media");
    let report_dir = paths.out.join("report");
    std::fs::create_dir_all(&media_dir)?;
    std::fs::create_dir_all(&report_dir)?;
    let progress_path = report_dir.join("media-progress.log");
    let progress = Arc::new(Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(&progress_path)?));
    let log = {
        let progress = progress.clone();
        move |line: String| {
            eprintln!("{line}");
            if let Ok(mut f) = progress.lock() {
                let _ = writeln!(f, "{line}");
            }
        }
    };
    log(format!("== media run started (workers {workers}{})", if filter.is_empty() { String::new() } else { format!(", filter {filter:?}") }));

    let tools = ffmpeg::Tools::detect()?;
    log(format!("ffmpeg: {} | encoders {:?}", tools.version, tools));

    // 1. Survey.
    let members = survey(&paths.extracted, &filter)?;
    let (mut sources, by_sha, kind_conflicts) = sources(&members);
    log(format!("survey: {} files, {} media files, {} unique sources ({:.1} s)", members.len(), by_sha.values().map(Vec::len).sum::<usize>(), sources.len(), t0.elapsed().as_secs_f64()));

    let by_path: HashMap<(String, String), usize> =
        members.iter().enumerate().map(|(i, m)| ((m.container.clone(), m.member.to_ascii_lowercase()), i)).collect();
    let mut by_base: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        if m.sha.is_some() {
            let b = m.member.rsplit('/').next().unwrap_or(&m.member).to_ascii_lowercase();
            by_base.entry(b).or_default().push(i);
        }
    }

    // 2. `.iax` references, and their root attributes as the original's metadata.
    let iax_idx: Vec<usize> = members.iter().enumerate().filter(|(_, m)| kind_of(m) == Some(Kind::Interactive)).map(|(i, _)| i).collect();
    let mut iax_meta: HashMap<String, serde_json::Value> = HashMap::new();
    let mut iax_refs = Vec::new();
    for &i in &iax_idx {
        let m = &members[i];
        let text = String::from_utf8_lossy(&std::fs::read(&m.abs)?).into_owned();
        let parsed = iax::parse(&text);
        let sha = m.sha.clone().unwrap();
        iax_meta.entry(sha.clone()).or_insert_with(|| json!({ "iax": parsed.root }));
        for r in parsed.references {
            let (resolution, target) = resolve_iax(&members, i, &r.name, &by_path, &by_base);
            iax_refs.push(IaxRef { iax_sha: sha.clone(), element: r.element, name: r.name, resolution, target });
        }
    }
    for s in &mut sources {
        s.meta = iax_meta.remove(&s.sha);
    }

    // 3. Catalog keys.
    let (catalog, catalog_summary) = catalog_rows(&paths.extracted, &members, &by_path)?;

    // 4. Static tables.
    let mut db = manifest::open(&media_dir.join("manifest.sqlite"))?;
    {
        let tx = db.transaction()?;
        tx.execute_batch("DELETE FROM file; DELETE FROM catalog_key; DELETE FROM iax_ref; DELETE FROM failure; DELETE FROM run;")?;
        {
            let mut st = tx.prepare(
                "INSERT INTO file(container, package, member_path, name, sha256, size, mime, kind, format, disposition) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            )?;
            for m in &members {
                let (Some(sha), Some(kind)) = (&m.sha, kind_of(m)) else { continue };
                let fmt = m.fmt.unwrap_or(Format::Other);
                let name = m.member.rsplit('/').next().unwrap_or(&m.member).to_ascii_lowercase();
                let disp = if kind.converts() { "converted" } else { "kept" };
                st.execute(rusqlite::params![m.container, m.package(), m.member, name, sha, m.size as i64, fmt.mime(), kind.name(), fmt.name(), disp])?;
            }
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO catalog_key(edition, tbl, key, package_index, package_file, local_id, container, member_path, sha256) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )?;
            for r in &catalog {
                st.execute(rusqlite::params![
                    r.edition,
                    r.table,
                    r.key,
                    r.package_index,
                    r.package_file,
                    r.local_id,
                    r.target.as_ref().map(|t| &t.0),
                    r.target.as_ref().map(|t| &t.1),
                    r.sha
                ])?;
            }
            let mut st = tx.prepare(
                "INSERT INTO iax_ref(iax_sha256, element, ref, resolution, container, member_path, sha256) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )?;
            // One set of rows per distinct .iax content (Kids copies are byte-identical).
            let mut seen = std::collections::HashSet::new();
            for r in &iax_refs {
                if !seen.insert((r.iax_sha.clone(), r.element.clone(), r.name.clone())) {
                    continue;
                }
                let t = r.target.map(|i| &members[i]);
                st.execute(rusqlite::params![r.iax_sha, r.element, r.name, r.resolution, t.map(|m| &m.container), t.map(|m| &m.member), t.and_then(|m| m.sha.as_ref())])?;
            }
            let mut st = tx.prepare("INSERT INTO run(key, value) VALUES (?1, ?2)")?;
            st.execute(rusqlite::params!["ffmpeg", tools.version])?;
            st.execute(rusqlite::params!["encoders", serde_json::to_string(&tools)?])?;
            st.execute(rusqlite::params!["filter", serde_json::to_string(&filter)?])?;
        }
        tx.commit()?;
    }

    // 5. Plan: skip outputs already present with the right hash.
    let store = Store::open(&media_dir)?;
    backfill_durations(&db, &store, workers, &log)?;
    let existing = manifest::load_derivatives(&db)?;
    let mut plan: Vec<(Source, Vec<Spec>)> = sources
        .par_iter()
        .map(|src| {
            let todo: Vec<Spec> = jobs::wanted(src, &tools)
                .into_iter()
                .filter(|sp| {
                    !existing.get(&(src.sha.clone(), sp.format.clone())).is_some_and(|row| {
                        row.recipe == sp.recipe && row.path == src.rel(sp) && store.verify(&row.path, row.bytes, &row.out_sha256)
                    })
                })
                .collect();
            (src.clone(), todo)
        })
        .filter(|(_, todo)| !todo.is_empty())
        .collect();
    plan.sort_by_key(|(s, _)| std::cmp::Reverse(s.cost()));
    // Extra caption tracks (`vtt-<class>`) are regenerated with `vtt`; drop the old ones first.
    for (src, todo) in &plan {
        if todo.iter().any(|t| t.format == "vtt") {
            for ((sha, fmt), row) in existing.iter().filter(|((sha, f), _)| *sha == src.sha && f.starts_with("vtt-")) {
                let _ = std::fs::remove_file(store.abs(&row.path));
                db.execute("DELETE FROM derivative WHERE sha256 = ?1 AND format = ?2", rusqlite::params![sha, fmt])?;
            }
        }
    }
    let skipped = sources.len() - plan.len();
    log(format!("plan: {} jobs to run, {} sources already complete ({:.1} s)", plan.len(), skipped, t0.elapsed().as_secs_f64()));
    if filter.is_empty() {
        // Drop rows of sources that are no longer on the disc survey.
        let live: std::collections::HashSet<&str> = sources.iter().map(|s| s.sha.as_str()).collect();
        let stale: Vec<&(String, String)> = existing.keys().filter(|(sha, _)| !live.contains(sha.as_str())).collect();
        for (sha, fmt) in stale {
            db.execute("DELETE FROM derivative WHERE sha256 = ?1 AND format = ?2", rusqlite::params![sha, fmt])?;
        }
    }

    // 6. Transcode on a bounded pool, writing rows as jobs finish.
    let total_jobs = plan.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed_outputs = AtomicUsize::new(0);
    let bytes_written = AtomicU64::new(0);
    let failures: Mutex<HashMap<String, Vec<(String, String)>>> = Mutex::new(HashMap::new());
    let (tx, rx) = std::sync::mpsc::channel::<Msg>();
    let t_jobs = Instant::now();
    let stop = AtomicUsize::new(0);
    let mut db = std::thread::scope(|scope| -> Result<rusqlite::Connection> {
        let writer = scope.spawn(move || -> Result<rusqlite::Connection> {
            let mut pending = 0;
            let mut last = Instant::now();
            db.execute_batch("BEGIN")?;
            for msg in rx {
                let Msg::Done { sha, outcome } = msg;
                for row in &outcome.rows {
                    manifest::put_derivative(&db, row)?;
                    pending += 1;
                }
                for (fmt, err) in &outcome.failures {
                    manifest::put_failure(&db, &sha, fmt, err)?;
                }
                if pending > 500 || last.elapsed() > Duration::from_secs(2) {
                    db.execute_batch("COMMIT; BEGIN")?;
                    pending = 0;
                    last = Instant::now();
                }
            }
            db.execute_batch("COMMIT")?;
            Ok(db)
        });
        let monitor = scope.spawn(|| {
            let mut last = Instant::now();
            while stop.load(Ordering::Relaxed) == 0 {
                std::thread::sleep(Duration::from_millis(250));
                if last.elapsed() < Duration::from_secs(20) {
                    continue;
                }
                last = Instant::now();
                let d = done.load(Ordering::Relaxed);
                let el = t_jobs.elapsed().as_secs_f64();
                let eta = if d > 0 { el / d as f64 * (total_jobs - d) as f64 } else { f64::NAN };
                log(format!(
                    "progress: {d}/{total_jobs} jobs, {} failed outputs, {:.1} MB written, {:.0} s elapsed, ETA {:.0} s (to resume: re-run `encarta-convert media`)",
                    failed_outputs.load(Ordering::Relaxed),
                    bytes_written.load(Ordering::Relaxed) as f64 / 1e6,
                    el,
                    eta
                ));
            }
        });
        let mut handles = Vec::new();
        for _ in 0..workers {
            let tx = tx.clone();
            let (plan, next, done, store, tools, failures, failed_outputs, bytes_written) =
                (&plan, &next, &done, &store, &tools, &failures, &failed_outputs, &bytes_written);
            handles.push(scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((src, todo)) = plan.get(i) else { break };
                    let outcome = jobs::run(src, todo, store, tools);
                    bytes_written.fetch_add(outcome.rows.iter().map(|r| r.bytes).sum(), Ordering::Relaxed);
                    if !outcome.failures.is_empty() {
                        failed_outputs.fetch_add(outcome.failures.len(), Ordering::Relaxed);
                        failures.lock().unwrap().entry(src.sha.clone()).or_default().extend(outcome.failures.iter().cloned());
                    }
                    let _ = tx.send(Msg::Done { sha: src.sha.clone(), outcome });
                    done.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        drop(tx);
        for h in handles {
            let _ = h.join();
        }
        stop.store(1, Ordering::Relaxed);
        let _ = monitor.join();
        writer.join().map_err(|_| anyhow::anyhow!("manifest writer panicked"))?
    })?;
    let failures = failures.into_inner().unwrap();
    log(format!("transcode: {} jobs in {:.1} s, {} failed outputs", total_jobs, t_jobs.elapsed().as_secs_f64(), failed_outputs.load(Ordering::Relaxed)));

    // 7. Director chart datasets (shockwave.md §6–§8).
    let interactives = write_chart_datasets(&paths.extracted, &media_dir)?;
    log(format!("interactives: {interactives}"));

    // 8. Member table and completeness report.
    let final_rows = manifest::load_derivatives(&db)?;
    write_report(&mut db, &report_dir, &members, &sources, &final_rows, &failures, &catalog_summary, &iax_refs, &interactives, ReportCtx {
        filter: &filter,
        tools: &tools,
        elapsed: t0.elapsed(),
        jobs_run: total_jobs,
        jobs_skipped: skipped,
        kind_conflicts,
    })?;
    // 9. Where the atlas's own photos and sounds are (media/atlas.rs; needs content.sqlite).
    let content_db = paths.out.join("content.sqlite");
    if content_db.is_file() {
        atlas::run(&content_db, &media_dir, &report_dir, &log)?;
    }
    log(format!("== media run finished in {:.1} s", t0.elapsed().as_secs_f64()));
    Ok(())
}

/// Rows written before output durations were measured get them now (one ffprobe each).
fn backfill_durations(db: &rusqlite::Connection, store: &Store, workers: usize, log: &dyn Fn(String)) -> Result<()> {
    let rows: Vec<DerivRow> = manifest::load_derivatives(db)?
        .into_values()
        .filter(|r| jobs::PROBED_FORMATS.contains(&r.format.as_str()) && !r.meta.as_deref().unwrap_or("").contains("\"probed\":true"))
        .collect();
    if rows.is_empty() {
        return Ok(());
    }
    let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build()?;
    let measured: Vec<(DerivRow, Option<u64>)> = pool.install(|| {
        rows.into_par_iter().map(|r| {
            let d = ffmpeg::probe(&store.abs(&r.path)).ok().and_then(|p| p.duration_ms);
            (r, d)
        }).collect()
    });
    db.execute_batch("BEGIN")?;
    let mut n = 0;
    for (mut r, d) in measured {
        let Some(d) = d else { continue };
        let mut meta: serde_json::Value = r.meta.as_deref().and_then(|m| serde_json::from_str(m).ok()).unwrap_or_else(|| json!({}));
        meta["probed"] = json!(true);
        r.meta = Some(meta.to_string());
        r.duration_ms = Some(d);
        manifest::put_derivative(db, &r)?;
        n += 1;
    }
    db.execute_batch("COMMIT")?;
    log(format!("backfill: measured the duration of {n} existing audio/video outputs"));
    Ok(())
}

/// The City Climate and Population Chart datasets, read from the Director casts with
/// `encarta_formats::director`, → `out/media/interactives/{city_climate,population}.json`.
fn write_chart_datasets(extracted: &Path, media_dir: &Path) -> Result<serde_json::Value> {
    use encarta_formats::director;
    let base = extracted.join("eit/EE/ENCARTA/SWALLDSK.EIT/dswMedia/ia");
    let out_dir = media_dir.join("interactives");
    std::fs::create_dir_all(&out_dir)?;
    let mut summary = serde_json::Map::new();
    let mut emit = |name: &str, folder: &str, data: Result<(serde_json::Value, Vec<String>)>| -> Result<()> {
        let source = format!("EE/ENCARTA/SWALLDSK.EIT/dswMedia/ia/{folder}");
        let entry = match data {
            Ok((value, issues)) => {
                let rel = format!("interactives/{name}.json");
                let tmp = media_dir.join(format!(".tmp/{name}.json"));
                std::fs::write(&tmp, serde_json::to_vec_pretty(&value)?)?;
                std::fs::rename(&tmp, media_dir.join(&rel))?;
                json!({ "path": rel, "source": source, "issues": issues })
            }
            Err(e) => json!({ "source": source, "error": format!("{e:#}") }),
        };
        summary.insert(name.to_string(), entry);
        Ok(())
    };
    let clim = base.join("clim_CCL");
    if clim.is_dir() {
        emit("city_climate", "clim_CCL", director::interactive::city_climate(&clim).and_then(|(d, i)| Ok((serde_json::to_value(d)?, i))))?;
    }
    let pop = base.join("pop_CPO");
    if pop.is_dir() {
        emit("population", "pop_CPO", director::interactive::population_chart(&pop).and_then(|(d, i)| Ok((serde_json::to_value(d)?, i))))?;
    }
    Ok(serde_json::Value::Object(summary))
}

struct ReportCtx<'a> {
    filter: &'a [String],
    tools: &'a ffmpeg::Tools,
    elapsed: Duration,
    jobs_run: usize,
    jobs_skipped: usize,
    kind_conflicts: usize,
}

#[allow(clippy::too_many_arguments)]
fn write_report(
    db: &mut rusqlite::Connection,
    report_dir: &Path,
    members: &[Member],
    sources: &[Source],
    rows: &HashMap<(String, String), DerivRow>,
    failures: &HashMap<String, Vec<(String, String)>>,
    catalog_summary: &serde_json::Value,
    iax_refs: &[IaxRef],
    interactives: &serde_json::Value,
    ctx: ReportCtx,
) -> Result<()> {
    let statuses: Vec<(String, Option<&'static str>, Option<String>)> = members.iter().map(|m| member_status(m, failures)).collect();
    {
        let tx = db.transaction()?;
        tx.execute("DELETE FROM member", [])?;
        let mut st = tx.prepare(
            "INSERT INTO member(container, member_path, size, format, status, owner, reason, sha256) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        for (m, (status, owner, reason)) in members.iter().zip(&statuses) {
            st.execute(rusqlite::params![m.container, m.member, m.size as i64, m.fmt.map(|f| f.name()), status, owner, reason, m.sha])?;
        }
        drop(st);
        tx.commit()?;
    }
    let mut tsv = std::io::BufWriter::new(std::fs::File::create(report_dir.join("media-members.tsv"))?);
    writeln!(tsv, "container\tmember_path\tsize\tformat\tstatus\towner\treason\tsha256")?;
    for (m, (status, owner, reason)) in members.iter().zip(&statuses) {
        writeln!(
            tsv,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            m.container,
            m.member,
            m.size,
            m.fmt.map_or("-", |f| f.name()),
            status,
            owner.unwrap_or(""),
            reason.as_deref().unwrap_or("").replace('\t', " "),
            m.sha.as_deref().unwrap_or("")
        )?;
    }
    tsv.flush()?;

    // Counts.
    let mut by_status: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let mut by_rule: BTreeMap<(String, String, String), (u64, u64, String)> = BTreeMap::new();
    let mut by_container: BTreeMap<&str, BTreeMap<String, u64>> = BTreeMap::new();
    let mut by_kind_ext: BTreeMap<(String, String, String), (u64, u64)> = BTreeMap::new();
    for (m, (status, owner, reason)) in members.iter().zip(&statuses) {
        let e = by_status.entry(status.as_str()).or_default();
        e.0 += 1;
        e.1 += m.size;
        let what = match (&m.disp, status.as_str()) {
            (Disposition::Media { kind, key }, _) => format!("{}{}", kind.name(), key.map(|k| format!(" (key {k:?})")).unwrap_or_default()),
            _ => reason.clone().unwrap_or_default(),
        };
        let r = by_rule
            .entry((status.clone(), owner.unwrap_or("").to_string(), if status == "failed" { kind_of(m).map_or(String::new(), |k| k.name().into()) } else { what }))
            .or_insert((0, 0, m.key()));
        r.0 += 1;
        r.1 += m.size;
        *by_container.entry(&m.container).or_default().entry(status.clone()).or_default() += 1;
        if let Some(k) = kind_of(m) {
            let ext = m.member.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
            let x = by_kind_ext.entry((k.name().into(), ext, m.fmt.map_or("-", |f| f.name()).into())).or_default();
            x.0 += 1;
            x.1 += m.size;
        }
    }
    let mut outputs: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for row in rows.values() {
        let o = outputs.entry(if row.format.starts_with("vtt") { "vtt" } else { row.format.as_str() }).or_default();
        o.0 += 1;
        o.1 += row.bytes;
    }
    let output_bytes: u64 = outputs.iter().filter(|(f, _)| **f != "original").map(|(_, v)| v.1).sum();
    let source_bytes: u64 = sources.iter().map(|s| s.size).sum();
    let mut src_kinds: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for s in sources {
        let k = src_kinds.entry(s.kind.name()).or_default();
        k.0 += 1;
        k.1 += s.size;
    }
    let member_index: HashMap<&str, Vec<String>> = {
        let mut m: HashMap<&str, Vec<String>> = HashMap::new();
        for mm in members {
            if let Some(s) = &mm.sha {
                m.entry(s.as_str()).or_default().push(mm.key());
            }
        }
        m
    };
    let mut failure_list = Vec::new();
    let mut not_applicable = Vec::new();
    for (sha, fs) in failures {
        for (fmt, err) in fs {
            let entry = json!({ "sha256": sha, "format": fmt, "error": err, "members": member_index.get(sha.as_str()).map(|v| v.iter().take(4).collect::<Vec<_>>()) });
            if err.starts_with("not applicable") { not_applicable.push(entry) } else { failure_list.push(entry) }
        }
    }
    failure_list.sort_by(|a, b| a["format"].as_str().cmp(&b["format"].as_str()).then(a["sha256"].as_str().cmp(&b["sha256"].as_str())));
    let list = |status: &str| -> Vec<serde_json::Value> {
        members
            .iter()
            .zip(&statuses)
            .filter(|(_, s)| s.0 == status)
            .map(|(m, s)| json!({ "container": m.container, "member_path": m.member, "size": m.size, "format": m.fmt.map(|f| f.name()), "reason": s.2 }))
            .collect()
    };
    let iax_resolutions = {
        let mut m: BTreeMap<&str, u64> = BTreeMap::new();
        for r in iax_refs {
            *m.entry(r.resolution).or_default() += 1;
        }
        m
    };
    let unresolved: Vec<serde_json::Value> = iax_refs
        .iter()
        .filter(|r| r.target.is_none())
        .take(200)
        .map(|r| json!({ "iax": member_index.get(r.iax_sha.as_str()).and_then(|v| v.first()), "element": r.element, "ref": r.name }))
        .collect();
    let report = json!({
        "generated_unix": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "partial": !ctx.filter.is_empty(),
        "filter": ctx.filter,
        "elapsed_s": ctx.elapsed.as_secs_f64(),
        "tools": ctx.tools,
        "totals": {
            "files": members.len(),
            "file_bytes": members.iter().map(|m| m.size).sum::<u64>(),
            "by_status": by_status.iter().map(|(k, v)| (k.to_string(), json!({ "files": v.0, "bytes": v.1 }))).collect::<serde_json::Map<_, _>>(),
            "unique_media_sources": sources.len(),
            "unique_media_source_bytes": source_bytes,
            "sources_by_kind": src_kinds.iter().map(|(k, v)| (k.to_string(), json!({ "sources": v.0, "bytes": v.1 }))).collect::<serde_json::Map<_, _>>(),
            "outputs": outputs.iter().map(|(k, v)| (k.to_string(), json!({ "files": v.0, "bytes": v.1 }))).collect::<serde_json::Map<_, _>>(),
            "derivative_bytes": output_bytes,
            "jobs_run": ctx.jobs_run,
            "sources_already_complete": ctx.jobs_skipped,
            "failed_outputs": failure_list.len(),
            "not_applicable_outputs": not_applicable.len(),
            "sources_with_conflicting_kinds": ctx.kind_conflicts,
            "unexplained": by_status.get("unexplained").map_or(0, |v| v.0),
        },
        "media_by_kind_extension_format": by_kind_ext.iter().map(|((k, e, f), v)| json!({ "kind": k, "extension": e, "sniffed": f, "files": v.0, "bytes": v.1 })).collect::<Vec<_>>(),
        "by_rule": by_rule.iter().map(|((s, o, w), v)| json!({ "status": s, "owner": o, "rule": w, "files": v.0, "bytes": v.1, "example": v.2 })).collect::<Vec<_>>(),
        "by_container": by_container,
        "catalog": catalog_summary,
        "interactives": interactives,
        "iax": { "references": iax_refs.len(), "by_resolution": iax_resolutions, "unresolved_sample": unresolved },
        "failures": failure_list,
        "not_applicable": not_applicable,
        "unexplained": list("unexplained"),
        "omitted": list("omitted"),
    });
    std::fs::write(report_dir.join("media.json"), serde_json::to_string_pretty(&report)?)?;

    // Summary on stderr.
    eprintln!("media summary:");
    for (k, v) in &by_status {
        eprintln!("  {k:<12} {:>8} files {:>14} bytes", v.0, v.1);
    }
    eprintln!("  unique media sources: {} ({} bytes)", sources.len(), source_bytes);
    for (k, v) in &outputs {
        eprintln!("  output {k:<10} {:>8} files {:>14} bytes", v.0, v.1);
    }
    eprintln!("  failed outputs: {}, not applicable: {}", failure_list.len(), not_applicable.len());
    Ok(())
}
