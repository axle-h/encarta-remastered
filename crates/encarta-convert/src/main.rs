//! encarta-convert [--extracted DIR] [--out DIR] <all|content|media|tours|atlas>
//!
//! `content` writes out/content.sqlite (`content-survey` writes the XML survey to out/report/); `media` writes out/media/ and out/media/manifest.sqlite;
//! `tours` writes out/tours/; `atlas` writes out/atlas/.

mod atlas;
mod content;
mod media;
mod search;
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--extracted" => paths.extracted = args.next().unwrap_or_default().into(),
            "--out" => paths.out = args.next().unwrap_or_default().into(),
            _ if command.is_none() => command = Some(arg),
            _ => bail!("unexpected argument {arg}"),
        }
    }
    match command.as_deref() {
        Some("content") => content::run(&paths),
        Some("media") => media::run(&paths),
        Some("content-survey") => content::survey::run(&paths),
        Some("tours") => tours::run(&paths),
        Some("atlas") => atlas::run(&paths),
        Some("search") => search::run(&paths),
        Some("all") => {
            content::run(&paths)?;
            media::run(&paths)?;
            tours::run(&paths)?;
            atlas::run(&paths)
        }
        _ => bail!("usage: encarta-convert [--extracted DIR] [--out DIR] <all|content|media|tours|atlas>"),
    }
}
