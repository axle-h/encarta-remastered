//! The Afterburner outer container (`XFIR` + `FGDM`/`FGDC`), spec `docs/formats/shockwave.md` §3.
//!
//! Layout: `Fver` (versions), `Fcdr` (compressor table), `ABMP` (resource map), `FGEI` (the
//! initial load segment followed by every other resource). All resources are inflated eagerly on
//! open; the files on the disc are at most a few hundred kilobytes.

use super::bytes::{Cursor, FourCC, inflate};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::collections::BTreeMap;

/// Movie (`.dcr`, codec `FGDM`) or cast (`.cct`, codec `FGDC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Movie,
    Cast,
}

/// How one resource is stored, taken from the compressor table entry it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// zlib ("Macromedia ziplib compression")
    Zlib,
    /// "Macromedia Fontmap compression": not zlib; bytes kept as stored
    FontMap,
    /// SWA audio ("SWA Decompressor Xtra"): bytes kept as stored
    Swa,
    /// no entry, or an unrecognised compressor: bytes kept as stored
    Raw,
}

#[derive(Debug, Clone, Serialize)]
pub struct Compressor {
    pub guid: String,
    pub name: String,
}

/// One entry of the `ABMP` resource map.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceEntry {
    pub id: u32,
    /// offset after the `FGEI` header; `None` for resources held in the initial load segment
    pub offset: Option<u32>,
    pub stored_size: u32,
    pub size: u32,
    pub compressor: u32,
    pub kind: FourCC,
}

#[derive(Debug, Clone, Serialize)]
pub struct Versions {
    /// `Fver` version (0x501 on the product files, 0x500 on the Director 7 diagnostic)
    pub fver: u32,
    pub imap: Option<u32>,
    pub director: Option<u32>,
    pub label: Option<String>,
}

/// A parsed Afterburner file: resources by id, already inflated.
#[derive(Debug, Clone)]
pub struct Container {
    pub kind: FileKind,
    pub versions: Versions,
    pub compressors: Vec<Compressor>,
    pub entries: Vec<ResourceEntry>,
    data: BTreeMap<u32, Vec<u8>>,
}

/// Resource id of the initial load segment.
const ILS_ID: u32 = 2;

impl Container {
    pub fn parse(file: &[u8]) -> Result<Self> {
        ensure!(file.len() >= 12, "file too short");
        ensure!(
            &file[0..4] == b"XFIR",
            "not a little-endian RIFX (no XFIR magic)"
        );
        let declared = u32::from_le_bytes(file[4..8].try_into().unwrap()) as usize;
        ensure!(declared + 8 <= file.len() + 8, "bad declared length");
        let kind = match &file[8..12] {
            b"MDGF" => FileKind::Movie,
            b"CDGF" => FileKind::Cast,
            other => bail!(
                "not an Afterburner file (codec {:?})",
                String::from_utf8_lossy(other)
            ),
        };
        let mut c = Cursor::new(file);
        c.seek(12);

        // Fver
        c.expect_tag("Fver")?;
        let len = c.varint()? as usize;
        let mut v = Cursor::new(c.take(len)?);
        let fver = v.varint()?;
        let (mut imap, mut director, mut label) = (None, None, None);
        if fver >= 0x401 {
            imap = Some(v.varint()?);
            director = Some(v.varint()?);
        }
        if fver >= 0x501 {
            let n = v.u8()? as usize;
            label = Some(String::from_utf8_lossy(v.take(n)?).into_owned());
        }
        let versions = Versions {
            fver,
            imap,
            director,
            label,
        };

        // Fcdr
        c.expect_tag("Fcdr")?;
        let len = c.varint()? as usize;
        let table = inflate(c.take(len)?).context("Fcdr")?;
        let compressors = parse_compressors(&table)?;

        // ABMP
        c.expect_tag("ABMP")?;
        let len = c.varint()? as usize;
        let mut a = Cursor::new(c.take(len)?);
        let _abmp_compression = a.varint()?;
        let abmp_size = a.varint()? as usize;
        let map = inflate(a.rest()).context("ABMP")?;
        ensure!(
            map.len() == abmp_size,
            "ABMP inflated to {} bytes, expected {abmp_size}",
            map.len()
        );
        let entries = parse_map(&map)?;

        // FGEI: everything after its varint is the resource area
        c.expect_tag("FGEI")?;
        let _ils_marker = c.varint()?;
        let base = c.pos();

        let mut out = Container {
            kind,
            versions,
            compressors,
            entries,
            data: BTreeMap::new(),
        };

        let ils = out
            .entries
            .iter()
            .find(|e| e.id == ILS_ID)
            .cloned()
            .context("no initial load segment (resource 2) in ABMP")?;
        let ils_off = base + ils.offset.context("ILS has no offset")? as usize;
        let ils_raw = file
            .get(ils_off..ils_off + ils.stored_size as usize)
            .context("ILS outside file")?;
        let ils_data = inflate(ils_raw).context("ILS")?;
        ensure!(ils_data.len() == ils.size as usize, "ILS size mismatch");
        out.split_ils(&ils_data)?;

        for e in out.entries.clone() {
            let Some(off) = e.offset else { continue };
            if e.id == ILS_ID {
                continue;
            }
            let start = base + off as usize;
            let raw = file
                .get(start..start + e.stored_size as usize)
                .with_context(|| format!("resource {} ({}) outside file", e.id, e.kind))?;
            let bytes = match out.compression(&e) {
                Compression::Zlib => {
                    let d =
                        inflate(raw).with_context(|| format!("resource {} ({})", e.id, e.kind))?;
                    ensure!(
                        d.len() == e.size as usize,
                        "resource {} inflated to {} bytes, expected {}",
                        e.id,
                        d.len(),
                        e.size
                    );
                    d
                }
                _ => raw.to_vec(),
            };
            out.data.insert(e.id, bytes);
        }
        Ok(out)
    }

    /// The initial load segment repeats {varint id, bytes}, sizes coming from the map.
    fn split_ils(&mut self, ils: &[u8]) -> Result<()> {
        let mut c = Cursor::new(ils);
        while !c.is_empty() {
            let id = c.varint()?;
            let e = self
                .entry(id)
                .with_context(|| format!("ILS holds resource {id}, which is not in ABMP"))?;
            ensure!(
                e.offset.is_none(),
                "resource {id} is both in the ILS and at an offset"
            );
            let size = e.size as usize;
            let bytes = c.take(size)?.to_vec();
            self.data.insert(id, bytes);
        }
        Ok(())
    }

    pub fn entry(&self, id: u32) -> Option<&ResourceEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn compression(&self, e: &ResourceEntry) -> Compression {
        let Some(c) = self.compressors.get(e.compressor as usize) else {
            return Compression::Raw;
        };
        let n = c.name.to_ascii_lowercase();
        if n.contains("ziplib") {
            Compression::Zlib
        } else if n.contains("fontmap") {
            Compression::FontMap
        } else if n.contains("swa") {
            Compression::Swa
        } else {
            Compression::Raw
        }
    }

    /// Inflated bytes of a resource.
    pub fn resource(&self, id: u32) -> Option<&[u8]> {
        self.data.get(&id).map(Vec::as_slice)
    }

    /// All resources of one type, in id order.
    pub fn of_kind(&self, kind: &str) -> impl Iterator<Item = (u32, &[u8])> + '_ {
        let k = FourCC::new(kind);
        self.entries
            .iter()
            .filter(move |e| e.kind == k)
            .filter_map(|e| Some((e.id, self.resource(e.id)?)))
    }

    /// The first resource of a type (`KEY*`, `MCsL`, `DRCF` …).
    pub fn first(&self, kind: &str) -> Option<(u32, &[u8])> {
        self.of_kind(kind).next()
    }
}

fn parse_compressors(b: &[u8]) -> Result<Vec<Compressor>> {
    let mut c = Cursor::new(b);
    let n = c.u16_le()? as usize;
    let mut guids = Vec::with_capacity(n);
    for _ in 0..n {
        guids.push(format_guid(c.take(16)?));
    }
    let mut out = Vec::with_capacity(n);
    for guid in guids {
        out.push(Compressor {
            guid,
            name: c.cstr()?,
        });
    }
    Ok(out)
}

fn format_guid(g: &[u8]) -> String {
    let d1 = u32::from_le_bytes(g[0..4].try_into().unwrap());
    let d2 = u16::from_le_bytes(g[4..6].try_into().unwrap());
    let d3 = u16::from_le_bytes(g[6..8].try_into().unwrap());
    let tail: String = g[8..].iter().map(|b| format!("{b:02X}")).collect();
    format!("{d1:08X}-{d2:04X}-{d3:04X}-{}-{}", &tail[..4], &tail[4..])
}

fn parse_map(b: &[u8]) -> Result<Vec<ResourceEntry>> {
    let mut c = Cursor::new(b);
    let _a = c.varint()?;
    let _b = c.varint()?;
    let n = c.varint()? as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let id = c.varint()?;
        let off = c.varint()? as i32;
        let stored_size = c.varint()?;
        let size = c.varint()?;
        let compressor = c.varint()?;
        let kind = FourCC::reversed(c.take(4)?);
        out.push(ResourceEntry {
            id,
            offset: (off >= 0).then_some(off as u32),
            stored_size,
            size,
            compressor,
            kind,
        });
    }
    ensure!(
        c.is_empty(),
        "{} trailing bytes after ABMP entries",
        c.remaining()
    );
    Ok(out)
}
