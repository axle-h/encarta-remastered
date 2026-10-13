//! akc info <file>                         header, dictionary and index summary
//! akc get <file> <local>                  one record's XML to stdout (raw bytes)
//! akc id <refId> [--kind content|data|relart] [--kids]
//!                                         resolve through the catalog, print the record's XML
//! akc key <key> [--kids]                  string-keyed AKC record (e.g. list/expert.xml)
//! akc dump <file> <outdir>                one <key>.xml file per record
//! akc check [--no-xml] <file>... | --all  decode everything; verify lengths, tiling, UTF-8,
//!                                         XML well-formedness and the spec's SHA-256 values
//!
//! Common option: --extracted DIR (default $ENCARTA_EXTRACTED, then `extracted`).
//! `--all` checks every .AKC under DIR/iso/EE/{ENCARTA,KIDS} and cross-checks each catalog
//! entry's id against the root `refid` of its record.

use anyhow::{Context, Result, bail};
use encarta_formats::akc::check::{self, FileReport, Options, hex};
use encarta_formats::akc::{AkcFile, Library};
use encarta_formats::catalog::{Catalog, Edition, Table};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    if let Err(e) = run() {
        let broken = e.chain().any(|c| {
            c.downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
        });
        if !broken {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

fn usage() -> anyhow::Error {
    anyhow::anyhow!(
        "usage: akc info <file> | get <file> <local> | id <refId> [--kind content|data|relart] [--kids] \
         | key <key> [--kids] | dump <file> <outdir> | check [--no-xml] <file>...|--all  [--extracted DIR]"
    )
}

struct Args {
    extracted: PathBuf,
    kids: bool,
    kind: Table,
    all: bool,
    xml: bool,
    rest: Vec<String>,
}

fn parse_args() -> Result<Args> {
    let mut a = Args {
        extracted: std::env::var_os("ENCARTA_EXTRACTED")
            .map(PathBuf::from)
            .unwrap_or("extracted".into()),
        kids: false,
        kind: Table::Content,
        all: false,
        xml: true,
        rest: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.as_str() {
            "--extracted" => a.extracted = it.next().context("--extracted needs a value")?.into(),
            "--kids" => a.kids = true,
            "--all" => a.all = true,
            "--no-xml" => a.xml = false,
            "--kind" => {
                a.kind = match it.next().as_deref() {
                    Some("content") => Table::Content,
                    Some("data") => Table::Data,
                    Some("relart") => Table::Relart,
                    other => bail!("--kind must be content, data or relart, not {other:?}"),
                }
            }
            _ => a.rest.push(s),
        }
    }
    Ok(a)
}

fn run() -> Result<()> {
    let a = parse_args()?;
    let rest: Vec<&str> = a.rest.iter().map(String::as_str).collect();
    let edition = if a.kids {
        Edition::Kids
    } else {
        Edition::Deluxe
    };
    match rest.as_slice() {
        ["info", file] => info(&AkcFile::open(file)?),
        ["get", file, key] => {
            let f = AkcFile::open(file)?;
            let bytes = f.record(key.parse().context("local id must be a number")?)?;
            std::io::stdout().lock().write_all(&bytes)?;
            Ok(())
        }
        ["id", id] => {
            let lib = Library::open(&a.extracted, edition)?;
            let id: u32 = id.parse().context("refId must be a number")?;
            let r = lib
                .get(id, a.kind)?
                .with_context(|| format!("{id} has no {:?} entry in the catalog", a.kind))?;
            eprintln!("{id} → {} record {}", r.file, r.local_id);
            std::io::stdout().lock().write_all(&r.bytes)?;
            Ok(())
        }
        ["key", key] => {
            let lib = Library::open(&a.extracted, edition)?;
            let r = lib
                .get_key(key)?
                .with_context(|| format!("{key} is not an AKC entry of the catalog"))?;
            eprintln!("{key} → {} record {}", r.file, r.local_id);
            std::io::stdout().lock().write_all(&r.bytes)?;
            Ok(())
        }
        ["dump", file, out] => dump(&AkcFile::open(file)?, Path::new(out)),
        ["check", files @ ..] if a.all || !files.is_empty() => {
            let opts = Options { xml: a.xml };
            if a.all {
                ensure_none(files)?;
                check_all(&a.extracted, &opts)
            } else {
                let mut ok = true;
                for f in files {
                    ok &= check_one(Path::new(f), &opts)?.0;
                }
                finish(ok)
            }
        }
        _ => Err(usage()),
    }
}

fn ensure_none(files: &[&str]) -> Result<()> {
    if files.is_empty() {
        Ok(())
    } else {
        bail!("--all takes no file arguments")
    }
}

fn finish(ok: bool) -> Result<()> {
    if ok {
        println!("OK");
        Ok(())
    } else {
        bail!("check failed")
    }
}

fn info(f: &AkcFile) -> Result<()> {
    let mut o = std::io::stdout().lock();
    let h = f.header();
    writeln!(o, "guid        {}", h.guid_string())?;
    writeln!(
        o,
        "flags       {:#x}{}",
        h.flags,
        if h.is_stub() {
            " (stub: no records)"
        } else {
            ""
        }
    )?;
    writeln!(
        o,
        "dictionary  offset {} length {}",
        h.dict_offset, h.dict_len
    )?;
    writeln!(
        o,
        "index       offset {} length {}",
        h.index_offset, h.index_len
    )?;
    writeln!(o, "label       {}", h.label)?;
    let Some(s) = f.shared() else {
        return Ok(());
    };
    let d = &s.header;
    writeln!(
        o,
        "shared dict C={} S={} A={} X={} words={} text={} maxlen={} (trailing bits {}, zero {})",
        s.capitalize as u8,
        s.self_closing as u8,
        d.a,
        d.x,
        d.wc,
        d.total,
        d.maxlen,
        s.trailing_bits.0,
        s.trailing_bits.1
    )?;
    let ih = f.index_header().unwrap();
    let direct = f.blocks().iter().filter(|b| b.direct).count();
    let ulen: u64 = f.records().iter().map(|r| r.ulen as u64).sum();
    let largest = f.records().iter().map(|r| r.ulen).max().unwrap_or(0);
    writeln!(
        o,
        "index       records={} sparse entries={} block entries={} blocks={} direct={} key width={}",
        f.len(),
        f.sparse_entries(),
        f.block_entries(),
        f.blocks().len(),
        direct,
        ih.key_width
    )?;
    let names = ["count", "ulen", "clen", "offset", "ntok", "unused"];
    for (set, fields) in [("direct", &ih.direct), ("shared", &ih.shared)] {
        for (n, fl) in names.iter().zip(fields.iter()) {
            writeln!(
                o,
                "  {set:<6} {n:<6} base {:>10} width {:>2} max {:>10}",
                fl.base, fl.width, fl.max
            )?;
        }
    }
    writeln!(o, "decoded     {ulen} bytes, largest record {largest}")?;
    Ok(())
}

fn dump(f: &AkcFile, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let t = Instant::now();
    f.for_each_record(|_, rec, bytes| {
        let p = out.join(format!("{}.xml", rec.key));
        std::fs::write(&p, bytes).with_context(|| format!("writing {}", p.display()))
    })?;
    eprintln!(
        "wrote {} records to {} in {:.1}s",
        f.len(),
        out.display(),
        t.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Checks one file; returns (ok, report, open file).
fn check_one(path: &Path, opts: &Options) -> Result<(bool, FileReport, AkcFile)> {
    let t = Instant::now();
    let f = AkcFile::open(path)?;
    let rep = check::check_file(&f, opts);
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let spec = check::compare_with_spec(&name, &rep, &f);
    let mut ok = rep.error_count == 0 && rep.non_utf8 == 0;
    let verdict = match &spec {
        None if f.header().is_stub() => "stub".to_string(),
        None => "no spec values".to_string(),
        Some(v) if v.is_empty() => "spec hashes match".to_string(),
        Some(v) => {
            ok = false;
            format!("SPEC MISMATCH: {}", v.join("; "))
        }
    };
    println!(
        "{name:<13} {:>7} records {:>11} bytes  largest {:>9}  blocks {:>5}  xml ok {:>7}  errors {}  {:.2}s  {}  {}",
        rep.records,
        rep.bytes,
        rep.largest,
        rep.blocks,
        rep.xml_ok,
        rep.error_count,
        t.elapsed().as_secs_f64(),
        hex(&rep.aggregate),
        verdict
    );
    if rep.non_utf8 > 0 {
        println!("  {} records are not valid UTF-8", rep.non_utf8);
    }
    for e in &rep.errors {
        println!("  {e}");
    }
    Ok((ok, rep, f))
}

/// Checked packages of one edition, by upper-case file name.
type Checked = HashMap<String, (FileReport, AkcFile)>;

fn check_all(extracted: &Path, opts: &Options) -> Result<()> {
    let t = Instant::now();
    let mut ok = true;
    let mut total = (0usize, 0u64);
    // file name (upper case) per edition → report
    let mut reports: Vec<(Edition, Checked)> = Vec::new();
    for (edition, dir) in [(Edition::Deluxe, "EE/ENCARTA"), (Edition::Kids, "EE/KIDS")] {
        let dir = extracted.join("iso").join(dir);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("akc")))
            .collect();
        files.sort();
        let mut map = HashMap::new();
        for p in files {
            let (o, rep, f) = check_one(&p, opts)?;
            ok &= o;
            total.0 += rep.records;
            total.1 += rep.bytes;
            let name = p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_uppercase();
            map.insert(name, (rep, f));
        }
        reports.push((edition, map));
    }
    println!(
        "total: {} records, {} bytes in {:.1}s",
        total.0,
        total.1,
        t.elapsed().as_secs_f64()
    );
    if opts.xml {
        for (edition, map) in &reports {
            let p = edition.paths(extracted);
            let Ok(cat) = Catalog::open(&p.catalog2, &p.ini) else {
                println!("{edition:?}: catalog not available, cross-check skipped");
                continue;
            };
            let (n, bad) = cross_check(&cat, map);
            println!(
                "{edition:?} catalog: {n} AKC entries, {} root refid mismatches",
                bad.len()
            );
            for b in bad.iter().take(10) {
                println!("  {b}");
            }
            ok &= bad.is_empty();
        }
    }
    finish(ok)
}

/// Every numeric catalog entry must land on a record whose root `refid` is the entry's id.
fn cross_check(cat: &Catalog, map: &Checked) -> (usize, Vec<String>) {
    let mut n = 0;
    let mut bad = Vec::new();
    for t in [Table::Content, Table::Data, Table::Relart] {
        for (id, loc) in cat.ecn(t).unwrap().iter() {
            let file = cat
                .package_file(loc.package)
                .unwrap_or("?")
                .to_ascii_uppercase();
            n += 1;
            let got = map
                .get(&file)
                .and_then(|(rep, f)| f.position(loc.local_id).map(|p| rep.root_refids[p]));
            if got != Some(Some(id)) {
                bad.push(format!(
                    "{t:?} {id} → {file} #{}: root refid {got:?}",
                    loc.local_id
                ));
            }
        }
    }
    (n, bad)
}
