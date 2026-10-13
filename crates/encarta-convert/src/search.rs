//! `encarta-convert search`: builds `out/search/` (tantivy index, autocomplete, spelling) from
//! `out/content.sqlite`. Everything is in the `encarta-search` crate; this writes the report.
//! Knobs: `SEARCH_THREADS` (indexing threads), `SEARCH_HEAP_MB` (total indexing memory).

use crate::Paths;
use anyhow::{Context, Result, ensure};
use encarta_search::BuildOptions;

pub fn run(paths: &Paths) -> Result<()> {
    let db = paths.out.join("content.sqlite");
    ensure!(db.is_file(), "{} not found: run `encarta-convert content` first", db.display());
    let mut opts = BuildOptions::default();
    if let Some(n) = std::env::var("SEARCH_THREADS").ok().and_then(|v| v.parse().ok()) {
        opts.threads = n;
        opts.heap_bytes = n * (192 << 20);
    }
    if let Some(mb) = std::env::var("SEARCH_HEAP_MB").ok().and_then(|v| v.parse::<usize>().ok()) {
        opts.heap_bytes = mb << 20;
    }
    let stats = encarta_search::content_db::build_from_content_db(&db, &paths.out.join("search"), &opts)?;
    let report = paths.out.join("report");
    std::fs::create_dir_all(&report)?;
    std::fs::write(report.join("search.json"), serde_json::to_vec_pretty(&stats)?).context("writing report/search.json")?;
    println!("{}", serde_json::to_string_pretty(&stats)?);
    Ok(())
}
