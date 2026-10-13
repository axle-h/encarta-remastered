//! Reads Afterburner Director files and Shockwave Audio (spec: docs/formats/shockwave.md).
//!
//! director ls <file> [--json]                    container chunks, cast libraries and members
//! director member <file> <cast> <n> [--out P]    extract one member (text/JSON to stdout, or to P)
//! director chunk <file> <id> [--out P]         one inflated resource, raw (hex dump without --out)
//! director fields <file>                         every field member as JSON (Lingo lists parsed)
//! director climate <dir> [--pretty]              City Climate dataset as JSON
//! director population <dir> [--pretty]           Population Chart dataset as JSON
//! director swa <file.swa> <out.mp3>              strip the SWA header, keep the MP3 stream
//! director check <dir>                           open every Director/SWA file below <dir>
//!
//! `<cast>` is a cast number (1-based, as listed by `ls`) or name. `<dir>` for climate/population
//! is the interactive's folder (`…/dswMedia/ia/clim_CCL`) or any folder above it
//! (`…/SWALLDSK.EIT`). Bitmaps whose palette lives in the movie take it from a `.dcr` next to the
//! cast or one folder up.

use anyhow::{Context, Result, bail};
use encarta_formats::director::extract::{self, PaletteSource};
use encarta_formats::director::{DirectorFile, MemberType, interactive, locate, sound, text};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: director ls <file> [--json] | member <file> <cast> <n> [--out P] | chunk <file> <id> [--out P] | fields <file> \
| climate <dir> [--pretty] | population <dir> [--pretty] | swa <in.swa> <out.mp3> | check <dir>";

fn read(p: &Path) -> Result<DirectorFile> {
    let b = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    DirectorFile::parse(&b).with_context(|| format!("parsing {}", p.display()))
}

fn emit<T: Serialize>(v: &T, pretty: bool) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    if pretty {
        serde_json::to_writer_pretty(&mut out, v)?;
    } else {
        serde_json::to_writer(&mut out, v)?;
    }
    writeln!(out)?;
    Ok(())
}

fn preview(s: &str, n: usize) -> String {
    let flat = s.replace('\n', "\\n");
    if flat.chars().count() > n {
        format!("{}…", flat.chars().take(n).collect::<String>())
    } else {
        flat
    }
}

fn ls(path: &Path, as_json: bool) -> Result<()> {
    let f = read(path)?;
    let c = &f.container;
    if as_json {
        let casts: Vec<_> = f
            .casts
            .iter()
            .map(|cl| {
                let members: Vec<_> = cl
                    .members
                    .iter()
                    .map(|m| {
                        let mut v = serde_json::to_value(m).unwrap_or_default();
                        if m.kind == MemberType::Script {
                            v["handlers"] = json!(cl.handler_names(c, m));
                        }
                        v
                    })
                    .collect();
                json!({"name": cl.name, "path": cl.path, "cast_id": cl.cast_id, "first_member": cl.first_member,
                       "last_member": cl.last_member, "external": cl.is_external(), "members": members})
            })
            .collect();
        return emit(
            &json!({"kind": c.kind, "versions": c.versions, "config": f.config, "compressors": c.compressors,
                    "chunks": c.entries, "casts": casts}),
            true,
        );
    }
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    writeln!(
        out,
        "{}: {:?}, Fver {:#x}, {}",
        path.display(),
        c.kind,
        c.versions.fver,
        c.versions.label.as_deref().unwrap_or("-")
    )?;
    if let Some(cfg) = &f.config {
        writeln!(
            out,
            "config: director {} stage {}x{} depth {} fps {} members {}..{}",
            cfg.director_version,
            cfg.width(),
            cfg.height(),
            cfg.bit_depth,
            cfg.frame_rate,
            cfg.members[0],
            cfg.members[1]
        )?;
    }
    for (i, comp) in c.compressors.iter().enumerate() {
        writeln!(out, "compressor {i}: {} {}", comp.guid, comp.name)?;
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &c.entries {
        *counts.entry(e.kind.to_string()).or_default() += 1;
    }
    writeln!(
        out,
        "chunks: {} ({})",
        c.entries.len(),
        counts
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    for e in &c.entries {
        let at = e.offset.map_or("ILS".to_owned(), |o| o.to_string());
        writeln!(
            out,
            "  {:5} {} {:>7} {:>7} -> {:>7} {:?}",
            e.id,
            e.kind,
            at,
            e.stored_size,
            e.size,
            c.compression(e)
        )?;
    }
    for (i, cl) in f.casts.iter().enumerate() {
        let where_ = if cl.is_external() {
            format!("external {}", cl.path)
        } else {
            format!("{} members", cl.members.len())
        };
        writeln!(
            out,
            "cast {} {:?} id {} members {}..{}: {where_}",
            i + 1,
            cl.name,
            cl.cast_id,
            cl.first_member,
            cl.last_member
        )?;
        for m in &cl.members {
            let data: Vec<String> = m.data.iter().map(|(k, id)| format!("{k}:{id}")).collect();
            let mut extra = String::new();
            match m.kind {
                MemberType::Field => {
                    if let Some(t) = m
                        .data_of("STXT")
                        .and_then(|id| f.resource(id))
                        .and_then(|b| text::field_text(b).ok())
                    {
                        extra = format!(" | {}", preview(&t, 70));
                    }
                }
                MemberType::Script => {
                    extra = format!(
                        " {:?} [{}]",
                        m.script_kind.unwrap(),
                        cl.handler_names(c, m).join(", ")
                    );
                }
                MemberType::Xtra => extra = format!(" xtra={}", m.xtra.as_deref().unwrap_or("?")),
                _ => {}
            }
            writeln!(
                out,
                "  {:4} {:<10} {:?} [{}]{extra}",
                m.number,
                m.kind.label(),
                m.name,
                data.join(" ")
            )?;
        }
    }
    Ok(())
}

/// Palettes from a movie next to (or one folder above) a cast file.
fn nearby_palettes(path: &Path) -> PaletteSource {
    let dir = path.parent().unwrap_or(Path::new("."));
    for d in [Some(dir), dir.parent()].into_iter().flatten() {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        let mut movies: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dcr")) && p != path)
            .collect();
        movies.sort();
        for m in movies {
            if let Ok(f) = read(&m) {
                let p = extract::palettes_of(&f);
                if !p.is_empty() {
                    return p;
                }
            }
        }
    }
    Vec::new()
}

fn member(path: &Path, cast: &str, n: u32, out: Option<&Path>) -> Result<()> {
    let f = read(path)?;
    let cl = f.cast(cast).with_context(|| format!("no cast {cast:?}"))?;
    let m = cl
        .member(n)
        .with_context(|| format!("no member {n} in cast {:?}", cl.name))?;
    let x = extract::extract_member(&f, cl, m, &nearby_palettes(path))?;
    match out {
        Some(p) => {
            let ext = x.extension();
            std::fs::write(p, x.into_bytes())
                .with_context(|| format!("writing {}", p.display()))?;
            eprintln!("wrote {} ({ext})", p.display());
        }
        None => match x {
            extract::Extracted::Text(t) => println!("{t}"),
            extract::Extracted::Json(v) => emit(&v, true)?,
            other => bail!(
                "member is binary ({}); pass --out <file>",
                other.extension()
            ),
        },
    }
    Ok(())
}

fn swa(input: &Path, output: &Path) -> Result<()> {
    let b = std::fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let (mp3, hdr, s) = sound::swa_to_mp3(&b)?;
    std::fs::write(output, mp3).with_context(|| format!("writing {}", output.display()))?;
    eprintln!(
        "{}: {} Hz, {} bit/s header; {} frames MPEG-{} layer {} {} kbit/s {}, {:.2} s",
        output.display(),
        hdr.sample_rate,
        hdr.bit_rate,
        s.frames,
        s.first.version,
        s.first.layer,
        s.first.bitrate_kbps,
        if s.first.mono { "mono" } else { "stereo" },
        s.duration_secs
    );
    Ok(())
}

fn check(dir: &Path) -> Result<bool> {
    let mut ok = true;
    for p in locate::director_files(dir) {
        let is_swa = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("swa"));
        let r = if is_swa {
            std::fs::read(&p)
                .map_err(anyhow::Error::from)
                .and_then(|b| {
                    sound::swa_to_mp3(&b).map(|(_, _, s)| {
                        format!("swa {} frames {:.2} s", s.frames, s.duration_secs)
                    })
                })
        } else {
            read(&p).map(|f| {
                let members: usize = f.casts.iter().map(|c| c.members.len()).sum();
                format!(
                    "{:?} {} chunks, {} casts, {} members",
                    f.container.kind,
                    f.container.entries.len(),
                    f.casts.len(),
                    members
                )
            })
        };
        match r {
            Ok(s) => println!("ok   {}: {s}", p.display()),
            Err(e) => {
                ok = false;
                println!("FAIL {}: {e:#}", p.display());
            }
        }
    }
    Ok(ok)
}

fn report(issues: &[String]) {
    for i in issues {
        eprintln!("issue: {i}");
    }
}

fn run(args: &[String]) -> Result<bool> {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let pos: Vec<&String> = {
        let mut v = Vec::new();
        let mut skip = false;
        for a in args {
            if skip {
                skip = false;
            } else if a == "--out" {
                skip = true;
            } else if !a.starts_with("--") {
                v.push(a);
            }
        }
        v
    };
    let opt = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let cmd = pos.first().map(|s| s.as_str()).unwrap_or("");
    let arg = |i: usize| {
        pos.get(i)
            .map(|s| PathBuf::from(s.as_str()))
            .with_context(|| USAGE.to_owned())
    };
    match cmd {
        "ls" => ls(&arg(1)?, flag("--json"))?,
        "member" => {
            let n: u32 = pos
                .get(3)
                .context(USAGE)?
                .parse()
                .context("member number")?;
            member(
                &arg(1)?,
                pos.get(2).context(USAGE)?,
                n,
                opt("--out").as_deref(),
            )?
        }
        "chunk" => {
            let f = encarta_formats::director::Container::parse(&std::fs::read(arg(1)?)?)?;
            let id: u32 = pos.get(2).context(USAGE)?.parse().context("resource id")?;
            let b = f
                .resource(id)
                .with_context(|| format!("no resource {id}"))?;
            match opt("--out") {
                Some(p) => std::fs::write(&p, b)?,
                None => {
                    let mut out = std::io::stdout().lock();
                    for (i, row) in b.chunks(16).enumerate() {
                        let hex: Vec<String> = row.iter().map(|x| format!("{x:02x}")).collect();
                        let asc: String = row
                            .iter()
                            .map(|&x| {
                                if x.is_ascii_graphic() || x == b' ' {
                                    x as char
                                } else {
                                    '.'
                                }
                            })
                            .collect();
                        writeln!(out, "{:06x}  {:<48} {asc}", i * 16, hex.join(" "))?;
                    }
                }
            }
        }
        "fields" => emit(&interactive::fields_json(&read(&arg(1)?)?)?, true)?,
        "climate" => {
            let d = arg(1)?;
            let dir = locate::find_interactive(&d, "clim_CCL")
                .with_context(|| format!("no clim_CCL under {}", d.display()))?;
            let (v, issues) = interactive::city_climate(&dir)?;
            report(&issues);
            emit(&v, flag("--pretty"))?;
            return Ok(issues.is_empty());
        }
        "population" => {
            let d = arg(1)?;
            let dir = locate::find_interactive(&d, "pop_CPO")
                .with_context(|| format!("no pop_CPO under {}", d.display()))?;
            let (v, issues) = interactive::population_chart(&dir)?;
            report(&issues);
            emit(&v, flag("--pretty"))?;
            return Ok(issues.is_empty());
        }
        "swa" => swa(&arg(1)?, &arg(2)?)?,
        "check" => return check(&arg(1)?),
        _ => bail!(USAGE),
    }
    Ok(true)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            let broken = e.chain().any(|c| {
                c.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            });
            if broken {
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
