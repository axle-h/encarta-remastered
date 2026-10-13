//! Finding Director files by the keys the app uses: `msencdata://~catalog~/dswMedia/<path>`
//! URLs, `dswmedia/<path>` keys in `generic.ecs` (which name the package: `SwShared.eit` or
//! `SwAllDsk.eit`), and the unpacked package folders under `extracted/eit/EE/{ENCARTA,KIDS}`.

use crate::catalog::{Catalog, Table, find_file_ci};
use std::path::{Path, PathBuf};

/// `msencdata://~catalog~/dswMedia/ia/x.cct` (or `msencData:` / `\` separators) → `dswmedia/ia/x.cct`.
pub fn key_from_url(url: &str) -> String {
    let s = url.replace('\\', "/");
    let lower = s.to_ascii_lowercase();
    let start = lower.find("dswmedia/").unwrap_or(0);
    lower[start..].trim_start_matches('/').to_owned()
}

/// Follow `rel` (`/`-separated) from `base`, matching each component ignoring ASCII case.
pub fn find_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = base.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        let exact = p.join(part);
        if exact.exists() {
            p = exact;
            continue;
        }
        let found = std::fs::read_dir(&p)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        p = found.path();
    }
    Some(p)
}

/// Resolve a `dswmedia/…` key through the catalog: the generic table names the package file,
/// which is looked up (ignoring case) in `package_dirs` as an unpacked folder.
pub fn resolve_with_catalog(cat: &Catalog, package_dirs: &[PathBuf], key: &str) -> Option<PathBuf> {
    let key = key_from_url(key);
    let r = cat
        .resolve_key(&key)
        .into_iter()
        .find(|r| r.table == Table::Generic)?;
    let package = find_file_ci(package_dirs, r.file.as_deref()?)?;
    find_ci(&package, &key).filter(|p| p.is_file())
}

/// Find the folder of one interactive (`clim_CCL`, `pop_CPO`) at or below `dir`: `dir` itself,
/// or `…/dswMedia/ia/<name>` under it (an unpacked `SWALLDSK.EIT`, its `dswMedia`, or an edition
/// folder holding the packages).
pub fn find_interactive(dir: &Path, name: &str) -> Option<PathBuf> {
    let is = |p: &Path| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
    };
    if is(dir) {
        return Some(dir.to_path_buf());
    }
    for rel in [
        format!("dswMedia/ia/{name}"),
        format!("ia/{name}"),
        name.to_owned(),
        format!("SWALLDSK.EIT/dswMedia/ia/{name}"),
    ] {
        if let Some(p) = find_ci(dir, &rel).filter(|p| p.is_dir()) {
            return Some(p);
        }
    }
    None
}

/// Every Director (`.dcr`, `.cct`, `.dir`, `.cst`, `.dxr`, `.cxt`) and `.swa` file below `dir`.
pub fn director_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .is_some_and(|x| {
                    matches!(
                        x.as_str(),
                        "dcr" | "cct" | "dir" | "cst" | "dxr" | "cxt" | "swa"
                    )
                })
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_to_key() {
        assert_eq!(
            key_from_url("msencdata://~catalog~/dswMedia/ia/clim_CCL/a/lclData.cct"),
            "dswmedia/ia/clim_ccl/a/lcldata.cct"
        );
        assert_eq!(
            key_from_url(r"msencData:dswMedia\shared\code\corescpt.cct"),
            "dswmedia/shared/code/corescpt.cct"
        );
    }
}
