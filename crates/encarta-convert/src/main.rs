//! encarta-convert [--extracted DIR] [--out DIR] <all|content|media|tours|atlas>
//!
//! `atlas-media` re-resolves the atlas's photos and sounds into the manifest (also the last step of `media`).
//! `content` writes out/content.sqlite (`content-survey` writes the XML survey to out/report/); `media` writes out/media/ and out/media/manifest.sqlite;
//! `tours` writes out/tours/; `atlas` writes out/atlas/; `soundfont [--sf2 FILE]` writes out/soundfont/ (the MIDI synth's GM soundfont).

mod atlas;
mod content;
mod media;
mod search;
mod soundfont;
mod tours;

use anyhow::{Result, bail};
use std::path::PathBuf;

pub struct Paths {
    pub extracted: PathBuf,
    pub out: PathBuf,
}

fn main() -> Result<()> {
    let mut paths = Paths {
        extracted: std::env::var_os("ENCARTA_EXTRACTED").map_or("extracted".into(), PathBuf::from),
        out: "out".into(),
    };
    let mut command = None;
    let mut sf2 = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--extracted" => paths.extracted = args.next().unwrap_or_default().into(),
            "--out" => paths.out = args.next().unwrap_or_default().into(),
            "--sf2" => sf2 = args.next().map(PathBuf::from),
            _ if command.is_none() => command = Some(arg),
            _ => bail!("unexpected argument {arg}"),
        }
    }
    match command.as_deref() {
        Some("content") => content::run(&paths),
        Some("media") => media::run(&paths),
        Some("atlas-media") => {
            let media_dir = paths.out.join("media");
            anyhow::ensure!(media_dir.join("manifest.sqlite").is_file(), "run `encarta-convert media` first");
            media::atlas::run(&paths.out.join("content.sqlite"), &media_dir, &paths.out.join("report"), &|l| eprintln!("{l}"))
        }
        Some("content-survey") => content::survey::run(&paths),
        Some("tours") => tours::run(&paths),
        Some("atlas") => atlas::run(&paths),
        Some("search") => search::run(&paths),
        Some("soundfont") => soundfont::run(&paths, sf2),
        Some("all") => {
            content::run(&paths)?;
            media::run(&paths)?;
            tours::run(&paths)?;
            atlas::run(&paths)
        }
        _ => bail!("usage: encarta-convert [--extracted DIR] [--out DIR] <all|content|media|atlas-media|tours|atlas|search|soundfont>"),
    }
}
