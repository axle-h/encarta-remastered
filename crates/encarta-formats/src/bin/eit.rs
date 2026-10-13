//! eit ls <file>                 list directory entries
//! eit info <file>               sections, transforms, LZX control data
//! eit cat <file> <name>         write one entry to stdout
//! eit extract <file> <outdir>   extract every non-internal entry

use anyhow::{Result, bail};
use encarta_formats::itol::Itol;
use std::io::Write;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, path) = match args.as_slice() {
        [_, cmd, path, ..] => (cmd.as_str(), Path::new(path)),
        _ => bail!("usage: eit <ls|info|cat|extract> <file> [arg]"),
    };
    let mut f = Itol::open(path)?;
    match cmd {
        "ls" => {
            for e in &f.entries {
                println!("{}\t{}\t{}\t{}", e.section, e.offset, e.size, e.name);
            }
        }
        "info" => {
            println!("content_offset {:#x}", f.content_offset);
            println!("entries {}", f.entries.len());
            for s in f.sections.clone() {
                println!("section {s}");
                if s == "Uncompressed" {
                    continue;
                }
                for t in f.transforms(&s)? {
                    println!("  transform {t}");
                }
                let control = f.section_storage(&s, "ControlData")?;
                println!("  control {:02x?}", control);
            }
        }
        "cat" => {
            let data = f.read(&args[3])?;
            std::io::stdout().write_all(&data)?;
        }
        "extract" => {
            let out = Path::new(&args[3]);
            let names: Vec<String> = f
                .entries
                .iter()
                .filter(|e| !e.name.starts_with("::") && !e.name.ends_with('/'))
                .map(|e| e.name.clone())
                .collect();
            for name in names {
                let data = f.read(&name)?;
                let dest = out.join(name.trim_start_matches('/'));
                std::fs::create_dir_all(dest.parent().unwrap())?;
                std::fs::write(dest, data)?;
            }
        }
        _ => bail!("unknown command {cmd}"),
    }
    Ok(())
}
