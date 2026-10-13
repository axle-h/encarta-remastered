//! MAD file header and table of contents (docs/formats/atlas-and-3d.md §1.1).

use anyhow::{Result, bail, ensure};
use serde::Serialize;

/// Value of the first header word in every supported MAD file.
pub const MAD_MAGIC: u32 = 0x2711;
const TOC_START: usize = 0x40;
const TOC_ENTRY: usize = 0x30;

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub version: String,
    /// The word at 0x20 (`0x0900A105` WLD, `0x0900A102` SWLD), meaning unconfirmed.
    pub stamp: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionEntry {
    pub name: String,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Toc {
    pub header: Header,
    pub sections: Vec<SectionEntry>,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Parses the header and section list. `file_len` is the length of the whole file, used to
/// check that every section lies inside it; `bytes` must hold at least the TOC.
pub fn parse_toc(bytes: &[u8], file_len: u64) -> Result<Toc> {
    ensure!(bytes.len() >= TOC_START, "MAD file too short for its header");
    let magic = u32_at(bytes, 0);
    if magic != MAD_MAGIC {
        bail!("not a supported MAD file (first word {magic:#x}, expected {MAD_MAGIC:#x})");
    }
    let count = u32_at(bytes, 4) as usize;
    let toc_end = TOC_START + count * TOC_ENTRY;
    ensure!(count < 4096 && bytes.len() >= toc_end, "MAD TOC of {count} entries is truncated");
    let version = cstr(&bytes[8..0x1C]);
    let stamp = u32_at(bytes, 0x20);

    let mut sections = Vec::with_capacity(count);
    let mut expected = toc_end as u64;
    for i in 0..count {
        let e = TOC_START + i * TOC_ENTRY;
        let offset = u32_at(bytes, e) as u64;
        let length = u32_at(bytes, e + 4) as u64;
        // The name is NUL-terminated; the rest of its 40 bytes is junk.
        let name = cstr(&bytes[e + 8..e + TOC_ENTRY]);
        ensure!(offset == expected, "section {name}: offset {offset} does not follow the previous section ({expected})");
        ensure!(offset + length <= file_len, "section {name} runs past the end of the file");
        expected = offset + length;
        sections.push(SectionEntry { name, offset, length });
    }
    Ok(Toc { header: Header { version, stamp }, sections })
}

/// Bytes up to the first NUL, as Latin-1.
pub(crate) fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(names: &[(&str, usize)]) -> Vec<u8> {
        let mut b = vec![0u8; TOC_START + names.len() * TOC_ENTRY];
        b[0..4].copy_from_slice(&MAD_MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&(names.len() as u32).to_le_bytes());
        b[8..13].copy_from_slice(b"TEST1");
        let mut off = b.len();
        for (i, (name, len)) in names.iter().enumerate() {
            let e = TOC_START + i * TOC_ENTRY;
            b[e..e + 4].copy_from_slice(&(off as u32).to_le_bytes());
            b[e + 4..e + 8].copy_from_slice(&(*len as u32).to_le_bytes());
            b[e + 8..e + 8 + name.len()].copy_from_slice(name.as_bytes());
            b[e + 9 + name.len()] = 0x55; // junk after the terminator
            off += len;
        }
        b.resize(off, 0xAA);
        b
    }

    #[test]
    fn parses_chain() {
        let b = synthetic(&[("Alpha", 3), ("Beta0409", 5)]);
        let toc = parse_toc(&b, b.len() as u64).unwrap();
        assert_eq!(toc.header.version, "TEST1");
        assert_eq!(toc.sections.len(), 2);
        assert_eq!(toc.sections[1].name, "Beta0409");
        assert_eq!(toc.sections[1].offset, toc.sections[0].offset + 3);
    }

    #[test]
    fn rejects_bad_magic_and_gaps() {
        let mut b = synthetic(&[("A", 3)]);
        assert!(parse_toc(&b, b.len() as u64 - 1).is_err());
        b[TOC_START] += 1;
        assert!(parse_toc(&b, b.len() as u64).is_err());
        b[0] = 0;
        assert!(parse_toc(&b, b.len() as u64).is_err());
    }
}
