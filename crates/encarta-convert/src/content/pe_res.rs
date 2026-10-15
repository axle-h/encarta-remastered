//! Resources of a Windows executable (the UI sounds and Kids artwork in `ENCARTAR.DLL`, the startup
//! sound in `ENCSPLSH.DLL`): walks the PE resource directory (type, name, language) and returns each
//! resource of one type with its numeric id or its name.

use anyhow::{Result, bail};

/// A resource name: a number, or a string such as `INTROSOUND`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResName {
    Id(u32),
    Name(String),
}

fn u16_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?) as usize)
}

fn u32_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Resources of one type as `(name, bytes)`.
pub type Resources = Vec<(ResName, Vec<u8>)>;

/// Every resource of type `kind` (`Id(2)` bitmaps, `Name("WAVE")` sounds …) of the PE file `d` as
/// `(name, bytes)`.
pub fn resources(d: &[u8], kind: &ResName) -> Result<Resources> {
    let bad = || anyhow::anyhow!("truncated PE file");
    let pe = u32_at(d, 0x3c).ok_or_else(bad)?;
    if d.get(pe..pe + 4) != Some(b"PE\0\0") {
        bail!("not a PE file");
    }
    let sections = u16_at(d, pe + 6).ok_or_else(bad)?;
    let opt_size = u16_at(d, pe + 20).ok_or_else(bad)?;
    let opt = pe + 24;
    let dirs = opt + if u16_at(d, opt).ok_or_else(bad)? == 0x20b { 112 } else { 96 };
    let rsrc_rva = u32_at(d, dirs + 16).ok_or_else(bad)?;
    if rsrc_rva == 0 {
        return Ok(Vec::new());
    }
    // (virtual size, virtual address, raw size, raw pointer) per section.
    let secs: Vec<[usize; 4]> = (0..sections)
        .map(|i| {
            let s = opt + opt_size + i * 40 + 8;
            Some([u32_at(d, s)?, u32_at(d, s + 4)?, u32_at(d, s + 8)?, u32_at(d, s + 12)?])
        })
        .collect::<Option<_>>()
        .ok_or_else(bad)?;
    let to_off = |rva: usize| secs.iter().find(|s| rva >= s[1] && rva < s[1] + s[0].max(s[2])).map(|s| rva - s[1] + s[3]);
    let base = to_off(rsrc_rva).ok_or_else(bad)?;

    // The entries of a directory table at `table` (offset from `base`): (name field, target field).
    let entries = |table: usize| -> Option<Vec<(usize, usize)>> {
        let o = base + table;
        let n = u16_at(d, o + 12)? + u16_at(d, o + 14)?;
        (0..n).map(|i| Some((u32_at(d, o + 16 + i * 8)?, u32_at(d, o + 20 + i * 8)?))).collect()
    };
    let name_of = |f: usize| -> Option<ResName> {
        if f & 0x8000_0000 == 0 {
            return Some(ResName::Id(f as u32));
        }
        let p = base + (f & 0x7fff_ffff);
        let n = u16_at(d, p)?;
        let units: Vec<u16> = (0..n).map(|i| u16_at(d, p + 2 + i * 2).map(|u| u as u16)).collect::<Option<_>>()?;
        Some(ResName::Name(String::from_utf16_lossy(&units)))
    };

    let mut out = Vec::new();
    for (ty, ty_ptr) in entries(0).ok_or_else(bad)? {
        if name_of(ty).as_ref() != Some(kind) || ty_ptr & 0x8000_0000 == 0 {
            continue;
        }
        for (name, name_ptr) in entries(ty_ptr & 0x7fff_ffff).ok_or_else(bad)? {
            let Some(name) = name_of(name) else { continue };
            if name_ptr & 0x8000_0000 == 0 {
                continue;
            }
            // First language only.
            let Some(&(_, leaf)) = entries(name_ptr & 0x7fff_ffff).ok_or_else(bad)?.first() else { continue };
            let (Some(rva), Some(size)) = (u32_at(d, base + leaf), u32_at(d, base + leaf + 4)) else { continue };
            let Some(off) = to_off(rva) else { continue };
            if let Some(b) = d.get(off..off + size) {
                out.push((name, b.to_vec()));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pe() {
        let wave = ResName::Name("WAVE".into());
        assert!(resources(&[0u8; 8], &wave).is_err());
        let mut d = vec![0u8; 0x100];
        d[0x3c] = 0x40;
        assert!(resources(&d, &wave).is_err());
    }
}
