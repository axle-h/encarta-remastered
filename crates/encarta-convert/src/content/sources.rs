//! The XML sources the content converter reads: every AKC package and the XML files inside EIT
//! containers (sidebars in `MD*.EIT/baggage`, the EDICT dictionaries, interactives `*.iax`).

use anyhow::{Context, Result};
use encarta_formats::akc::AkcFile;
use encarta_formats::catalog::{Catalog, Edition, Table};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub enum SourceKind {
    /// An AKC package; `ids[local]` is the record's catalog id (or string key) when catalogued.
    Akc {
        file: Arc<AkcFile>,
        ids: HashMap<u32, String>,
    },
    /// Loose XML files extracted from EIT containers: `(id, path)`.
    Files(Vec<(String, PathBuf)>),
}

pub struct Source {
    /// `akc/ContStd`, `md/MDSTD.EIT`, `edict/DICTA`, `iax/SWQUIZ.EIT` …
    pub label: String,
    pub kind: SourceKind,
}

/// Decodes EIT XML bytes to text: UTF-8 (with or without BOM) unless the declaration names a
/// single-byte Windows code page.
pub fn decode_xml_bytes(b: &[u8]) -> String {
    let head = &b[..b.len().min(100)];
    let head_s = String::from_utf8_lossy(head).to_ascii_lowercase();
    if head_s.contains("windows-1252") || head_s.contains("iso-8859-1") {
        let (s, _, _) = encoding_rs::WINDOWS_1252.decode(b);
        return s.into_owned();
    }
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (s, _, _) = encoding_rs::WINDOWS_1252.decode(b);
            s.into_owned()
        }
    }
}

impl Source {
    /// Folds over every record in parallel. `fold` gets the record id and its XML text.
    pub fn par_fold<T, I, F, R>(&self, init: I, fold: F, reduce: R) -> Result<T>
    where
        T: Send,
        I: Fn() -> T + Sync + Send,
        F: Fn(T, &str, &str) -> T + Sync + Send,
        R: Fn(T, T) -> T + Sync + Send,
    {
        match &self.kind {
            SourceKind::Akc { file, ids } => {
                let records = file.records();
                (0..file.blocks().len() as u32)
                    .into_par_iter()
                    .try_fold(&init, |mut acc, b| -> Result<T> {
                        let d = file.decode_block(b)?;
                        for &m in &file.blocks()[b as usize].members {
                            let rec = &records[m as usize];
                            let bytes = file.render(&d, rec)?;
                            let text = encarta_formats::akc::xml_text(&bytes)?;
                            let id = ids
                                .get(&rec.key)
                                .cloned()
                                .unwrap_or_else(|| format!("local:{}", rec.key));
                            acc = fold(acc, &id, text);
                        }
                        Ok(acc)
                    })
                    .try_reduce(&init, |a, b| Ok(reduce(a, b)))
            }
            SourceKind::Files(files) => files
                .par_iter()
                .try_fold(&init, |acc, (id, path)| -> Result<T> {
                    let b = std::fs::read(path)
                        .with_context(|| format!("reading {}", path.display()))?;
                    Ok(fold(acc, id, &decode_xml_bytes(&b)))
                })
                .try_reduce(&init, |a, b| Ok(reduce(a, b))),
        }
    }

    pub fn len(&self) -> usize {
        match &self.kind {
            SourceKind::Akc { file, .. } => file.len(),
            SourceKind::Files(f) => f.len(),
        }
    }
}

/// Opens an edition's catalog.
pub fn open_catalog(extracted: &Path, edition: Edition) -> Result<Catalog> {
    let p = edition.paths(extracted);
    Catalog::open(&p.catalog2, &p.ini)
}

/// `local id → catalog id` for one package slot, from the three numeric tables and generic.ecs.
pub fn package_ids(cat: &Catalog, package: u8) -> HashMap<u32, String> {
    let mut m = HashMap::new();
    for t in [Table::Content, Table::Data, Table::Relart] {
        for (id, loc) in cat.ecn(t).unwrap().iter() {
            if loc.package == package {
                m.insert(loc.local_id, id.to_string());
            }
        }
    }
    for (key, loc) in cat.generic.iter() {
        if loc.package == package {
            m.insert(loc.local_id, key.to_string());
        }
    }
    m
}

/// Every AKC package used by either catalog, plus the uncatalogued ones found on disc.
pub fn akc_sources(extracted: &Path) -> Result<Vec<Source>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for edition in [Edition::Deluxe, Edition::Kids] {
        let cat = open_catalog(extracted, edition)?;
        let dirs = edition.paths(extracted).package_dirs;
        for p in cat.packages.iter().flatten() {
            let Some(name) = &p.file else { continue };
            if !name.to_ascii_lowercase().ends_with(".akc") {
                continue;
            }
            let stem = name[..name.len() - 4].to_string();
            if !seen.insert(stem.to_ascii_lowercase()) {
                continue; // DataESK is listed by both catalogs (identical file in each directory)
            }
            let path = encarta_formats::catalog::find_file_ci(&dirs, name)
                .with_context(|| format!("{name} not found"))?;
            let file = Arc::new(AkcFile::open(&path)?);
            out.push(Source {
                label: format!("akc/{stem}"),
                kind: SourceKind::Akc {
                    ids: package_ids(&cat, p.index),
                    file,
                },
            });
        }
    }
    Ok(out)
}

fn list_files(dir: &Path, pred: &dyn Fn(&str) -> bool, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut v: Vec<_> = rd.flatten().collect();
    v.sort_by_key(|e| e.file_name());
    for e in v {
        let p = e.path();
        if p.is_dir() {
            list_files(&p, pred, out);
        } else {
            let name = e.file_name().to_string_lossy().to_string();
            if pred(&name) {
                out.push((name, p));
            }
        }
    }
}

/// The EIT containers (by directory) under `extracted/eit` matching `pred`.
fn eit_dirs(extracted: &Path, pred: &dyn Fn(&str) -> bool) -> Vec<(Edition, PathBuf)> {
    let eit = extracted.join("eit");
    let mut out = Vec::new();
    for (ed, d) in [
        (Edition::Deluxe, eit.join("CONTENT")),
        (Edition::Deluxe, eit.join("EE/ENCARTA")),
        (Edition::Kids, eit.join("EE/KIDS/CONTENT")),
        (Edition::Kids, eit.join("EE/KIDS")),
    ] {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut v: Vec<_> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| pred(n))
            .collect();
        v.sort();
        out.extend(v.into_iter().map(|n| (ed, d.join(n))));
    }
    out
}

/// Sidebar/text XML in `MD*.EIT/baggage`, dictionaries in `EDICT/*.EIT`, interactives `*.iax`.
pub fn eit_sources(extracted: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    let is_xml = |n: &str| n.to_ascii_lowercase().ends_with(".xml") && !n.starts_with('_');
    for (ed, dir) in eit_dirs(extracted, &|n| n.to_ascii_uppercase().starts_with("MD")) {
        let mut files = Vec::new();
        list_files(&dir.join("baggage"), &is_xml, &mut files);
        if !files.is_empty() {
            out.push(Source {
                label: format!("md/{}{}", kids_prefix(ed), dir.file_name().unwrap().to_string_lossy()),
                kind: SourceKind::Files(files),
            });
        }
    }
    let edict = extracted.join("eit/EE/ENCARTA/EDICT");
    if let Ok(rd) = std::fs::read_dir(&edict) {
        let mut v: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        v.sort();
        for d in v {
            let mut files = Vec::new();
            list_files(
                &d,
                &|n: &str| n.bytes().all(|b| b.is_ascii_digit()) || n == "$_EDictSignature2007_$",
                &mut files,
            );
            if files.is_empty() {
                continue;
            }
            let stem = d.file_name().unwrap().to_string_lossy().replace(".EIT", "");
            out.push(Source {
                label: format!("edict/{stem}"),
                kind: SourceKind::Files(files),
            });
        }
    }
    for (ed, dir) in eit_dirs(extracted, &|n| n.to_ascii_uppercase().starts_with("SW")) {
        let mut files = Vec::new();
        list_files(&dir, &|n: &str| n.to_ascii_lowercase().ends_with(".iax"), &mut files);
        if !files.is_empty() {
            out.push(Source {
                label: format!("iax/{}{}", kids_prefix(ed), dir.file_name().unwrap().to_string_lossy()),
                kind: SourceKind::Files(files),
            });
        }
    }
    out
}

fn kids_prefix(ed: Edition) -> &'static str {
    match ed {
        Edition::Kids => "KIDS/",
        Edition::Deluxe => "",
    }
}

pub fn all_xml_sources(extracted: &Path) -> Result<Vec<Source>> {
    let mut v = akc_sources(extracted)?;
    v.extend(eit_sources(extracted));
    Ok(v)
}
