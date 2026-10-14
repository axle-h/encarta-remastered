//! TrueType → WOFF2 (W3C WOFF 2.0): every table kept as is (the "null transform" for `glyf`/`loca`)
//! and the whole table data compressed as one Brotli stream. Smaller than the TTF, readable by every
//! browser; the converter keeps the TTF beside it for Ruffle.

use anyhow::{Result, bail};

/// The 63 table tags WOFF2 abbreviates to an index, in index order.
const KNOWN: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm", b"glyf", b"loca", b"prep",
    b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern", b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE",
    b"GDEF", b"GPOS", b"GSUB", b"EBSC", b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt",
    b"avar", b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar", b"gvar", b"hsty", b"just", b"lcar",
    b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat", b"Gloc", b"Feat", b"Sill",
];

fn be16(b: &[u8], o: usize) -> Result<usize> {
    b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize).ok_or_else(|| anyhow::anyhow!("truncated font"))
}
fn be32(b: &[u8], o: usize) -> Result<usize> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as usize).ok_or_else(|| anyhow::anyhow!("truncated font"))
}

/// `UIntBase128`: big-endian groups of 7 bits, high bit set on all but the last byte.
fn base128(out: &mut Vec<u8>, v: u32) {
    let mut groups = vec![(v & 0x7f) as u8];
    let mut v = v >> 7;
    while v > 0 {
        groups.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    groups.reverse();
    out.extend(groups);
}

/// Encodes a single-font TrueType/OpenType file (not a collection) as WOFF2.
pub fn encode(sfnt: &[u8]) -> Result<Vec<u8>> {
    let flavor = be32(sfnt, 0)?;
    if flavor == 0x7474_6366 {
        bail!("font collections are not supported");
    }
    let n = be16(sfnt, 4)?;
    let mut dir = Vec::new();
    let mut data = Vec::new();
    let mut total = 12 + 16 * n;
    for i in 0..n {
        let e = 12 + 16 * i;
        let tag: [u8; 4] = sfnt.get(e..e + 4).ok_or_else(|| anyhow::anyhow!("truncated font"))?.try_into()?;
        let (off, len) = (be32(sfnt, e + 8)?, be32(sfnt, e + 12)?);
        let table = sfnt.get(off..off + len).ok_or_else(|| anyhow::anyhow!("table {} out of range", String::from_utf8_lossy(&tag)))?;
        // glyf and loca (indices 10, 11) default to the transformed form; version 3 says "as is".
        let flags = match KNOWN.iter().position(|k| **k == tag) {
            Some(ix) if ix == 10 || ix == 11 => ix as u8 | 0xc0,
            Some(ix) => ix as u8,
            None => 63,
        };
        dir.push(flags);
        if flags == 63 {
            dir.extend(tag);
        }
        base128(&mut dir, len as u32);
        data.extend(table);
        total += len.next_multiple_of(4);
    }
    let mut comp = Vec::new();
    let params = brotli::enc::BrotliEncoderParams { quality: 11, lgwin: 22, ..Default::default() };
    brotli::BrotliCompress(&mut data.as_slice(), &mut comp, &params)?;
    let header_len = 48 + dir.len();
    let length = (header_len + comp.len()).next_multiple_of(4);
    let mut out = Vec::with_capacity(length);
    out.extend(b"wOF2");
    out.extend((flavor as u32).to_be_bytes());
    out.extend((length as u32).to_be_bytes());
    out.extend((n as u16).to_be_bytes());
    out.extend([0, 0]);
    out.extend((total as u32).to_be_bytes());
    out.extend((comp.len() as u32).to_be_bytes());
    out.extend([0, 1, 0, 0]); // font version 1.0
    out.extend([0u8; 20]); // no metadata, no private data
    out.extend(dir);
    out.extend(comp);
    out.resize(length, 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal two-table sfnt: `head`-less on purpose, the encoder only moves bytes.
    fn sample() -> Vec<u8> {
        let tables: [(&[u8; 4], Vec<u8>); 2] = [(b"glyf", vec![7; 301]), (b"ZZZZ", vec![1, 2, 3])];
        let mut f = Vec::new();
        f.extend([0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0]);
        let mut off = 12 + 32;
        for (tag, t) in &tables {
            f.extend(*tag);
            f.extend([0; 4]);
            f.extend((off as u32).to_be_bytes());
            f.extend((t.len() as u32).to_be_bytes());
            off += t.len();
        }
        for (_, t) in &tables {
            f.extend(t);
        }
        f
    }

    #[test]
    fn round_trips_tables() {
        let w = encode(&sample()).unwrap();
        assert_eq!(&w[..4], b"wOF2");
        assert_eq!(w.len() % 4, 0);
        assert_eq!(be32(&w, 8).unwrap(), w.len());
        assert_eq!(be16(&w, 12).unwrap(), 2);
        let clen = be32(&w, 20).unwrap();
        // directory: glyf as null-transform (0xca), length 301 = 0x82 0x2d; then an explicit tag.
        assert_eq!(&w[48..51], &[0xca, 0x82, 0x2d]);
        assert_eq!(&w[51..57], &[63, b'Z', b'Z', b'Z', b'Z', 3]);
        let mut raw = Vec::new();
        brotli::BrotliDecompress(&mut &w[57..57 + clen], &mut raw).unwrap();
        assert_eq!(raw.len(), 304);
        assert_eq!(&raw[301..], &[1, 2, 3]);
    }

    #[test]
    fn base128_values() {
        let mut v = Vec::new();
        base128(&mut v, 63);
        base128(&mut v, 128);
        base128(&mut v, 16384);
        assert_eq!(v, [63, 0x81, 0, 0x81, 0x80, 0]);
    }
}
