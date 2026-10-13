//! Dumps the small Encarta data formats as JSON on stdout.
//!
//! smallfmt timeline <TIMELINE.DAT>
//! smallfmt mindmaze <MINDMAZE.DB> <MINDMAZE.IDX>
//! smallfmt p2m      <P2MTBL.DAT>
//! smallfmt sdb      <MSWORLD.SDB>
//! smallfmt quiz     <Dn.TXT> [<DnPLACES.TXT>]
//! smallfmt catalog  <dir with BTREE and DATA>      (any $WW$CTCATALOG: key + property strings)
//! smallfmt media    <MSWORLD.ITR dir or its $WW$CTCATALOG dir>
//! smallfmt lex      <file.LEX>                     (header, scan statistics and the sorted word list)
//!
//! Add `--pretty` anywhere for indented JSON.

use anyhow::{Context, Result, bail};
use encarta_formats::{itir_catalog, lex, mindmaze, msworld, p2m, timeline};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: smallfmt <timeline|mindmaze|p2m|sdb|quiz|catalog|media|lex> <file...> [--pretty]";

fn read(p: &str) -> Result<Vec<u8>> {
    std::fs::read(p).with_context(|| format!("reading {p}"))
}

/// Accepts either a directory holding BTREE/DATA, or one containing `$WW$CTCATALOG/`.
fn catalog_dir(p: &str) -> PathBuf {
    let p = Path::new(p);
    let nested = p.join("$WW$CTCATALOG");
    if nested.join("BTREE").is_file() { nested } else { p.to_path_buf() }
}

fn scan_catalog(p: &str) -> Result<Vec<itir_catalog::CatalogEntry>> {
    let dir = catalog_dir(p);
    let btree = std::fs::read(dir.join("BTREE")).with_context(|| format!("reading {}/BTREE", dir.display()))?;
    let data = std::fs::read(dir.join("DATA")).with_context(|| format!("reading {}/DATA", dir.display()))?;
    itir_catalog::scan(&btree, &data)
}

fn emit<T: Serialize>(value: &T, pretty: bool) -> Result<()> {
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    let written = if pretty { serde_json::to_writer_pretty(&mut w, value) } else { serde_json::to_writer(&mut w, value) };
    let result = written.map_err(std::io::Error::from).and_then(|()| writeln!(w)).and_then(|()| w.flush());
    match result {
        // A closed pipe (e.g. `| head`) is not an error for a dump tool.
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

#[derive(Serialize)]
struct LexDump {
    header: lex::Header,
    stats: lex::ScanStats,
    words: Vec<String>,
}

#[derive(Serialize)]
struct Quiz {
    settings: msworld::QuizSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    places: Option<Vec<msworld::QuizPlace>>,
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let pretty = args.iter().any(|a| a == "--pretty");
    args.retain(|a| a != "--pretty");
    let (cmd, files) = args.split_first().context(USAGE)?;
    let need = |n: usize| -> Result<()> {
        if files.len() < n {
            bail!("{cmd}: expected {n} file argument(s)\n{USAGE}");
        }
        Ok(())
    };
    match cmd.as_str() {
        "timeline" => {
            need(1)?;
            emit(&timeline::parse(&read(&files[0])?)?, pretty)
        }
        "mindmaze" => {
            need(2)?;
            emit(&mindmaze::parse(&read(&files[0])?, &read(&files[1])?)?, pretty)
        }
        "p2m" => {
            need(1)?;
            emit(&p2m::parse(&read(&files[0])?)?, pretty)
        }
        "sdb" => {
            need(1)?;
            emit(&msworld::parse_sdb(&read(&files[0])?)?, pretty)
        }
        "quiz" => {
            need(1)?;
            let settings = msworld::parse_quiz_settings(&read(&files[0])?)?;
            let places = files.get(1).map(|p| read(p).and_then(|b| msworld::parse_quiz_places(&b))).transpose()?;
            emit(&Quiz { settings, places }, pretty)
        }
        "catalog" => {
            need(1)?;
            emit(&scan_catalog(&files[0])?, pretty)
        }
        "media" => {
            need(1)?;
            emit(&msworld::media_records(&scan_catalog(&files[0])?)?, pretty)
        }
        "lex" => {
            need(1)?;
            let l = lex::parse(&read(&files[0])?)?;
            let stats = l.scan()?;
            // traversal order is not alphabetical
            let mut words = l.words()?;
            words.sort();
            emit(&LexDump { header: l.header, stats, words }, pretty)
        }
        _ => bail!("unknown command {cmd:?}\n{USAGE}"),
    }
}
