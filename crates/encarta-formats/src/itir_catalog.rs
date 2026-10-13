//! Leaf scanner for the ITIR `$WW$CTCATALOG` B-tree (`BTREE` + `DATA` streams) found in
//! `MSWORLD.ITR` and `TIMELINE.EIT`. Spec: `docs/formats/msworld.md` §3.
//!
//! Only the catalog layout (u32 keys, property blocks of UTF-16 strings) is handled; the
//! `$WW…` word wheels and `$FX…` full-text indexes use other layouts.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

const LEAF_MAGIC: &[u8; 4] = b"AOLL";
const INDEX_MAGIC: &[u8; 4] = b"AOLI";
const LEAF_HEADER_LEN: usize = 0x30;
const NO_PAGE: u64 = u64::MAX;

/// One catalog entry: its B-tree key and the strings of its property block, in
/// property order (property 0 first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    pub key: u32,
    /// Bit 7 = property 0 present, bit 6 = property 1, ...
    pub present_mask: u8,
    pub properties: Vec<String>,
    /// True when the property block was stored inline in the BTREE page, false when in DATA.
    pub inline: bool,
}

/// Geometry read from the BTREE header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Geometry {
    pub page_size: usize,
    pub first_page: usize,
    pub page_count: usize,
}

fn u32_at(b: &[u8], off: usize) -> Result<u32> {
    let s = b.get(off..off + 4).with_context(|| format!("u32 at {off:#x} out of range"))?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

fn u64_at(b: &[u8], off: usize) -> Result<u64> {
    let s = b.get(off..off + 8).with_context(|| format!("u64 at {off:#x} out of range"))?;
    Ok(u64::from_le_bytes(s.try_into().unwrap()))
}

/// Reads the page geometry from the `ITOLITLS` header of a BTREE stream.
pub fn geometry(btree: &[u8]) -> Result<Geometry> {
    ensure!(btree.starts_with(b"ITOLITLS"), "BTREE does not start with ITOLITLS");
    let page_size = u32_at(btree, 0x124)? as usize;
    let first_page = u64_at(btree, 0x144)? as usize;
    let area_end = u64_at(btree, 0x48)? as usize;
    ensure!(page_size >= LEAF_HEADER_LEN && page_size.is_power_of_two(), "odd page size {page_size:#x}");
    ensure!(first_page <= area_end && area_end <= btree.len(), "page area {first_page:#x}..{area_end:#x} outside file");
    ensure!((area_end - first_page).is_multiple_of(page_size), "page area is not a whole number of pages");
    Ok(Geometry { page_size, first_page, page_count: (area_end - first_page) / page_size })
}

/// Little-endian base-128 (least significant group first).
fn varint_le(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*pos).context("truncated varint")?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("varint too long")
}

/// Big-endian base-128 (most significant group first), as in CHM "ENCINT".
fn encint(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for _ in 0..10 {
        let byte = *b.get(*pos).context("truncated encint")?;
        *pos += 1;
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("encint too long")
}

/// Decodes a property block: `u32 length, u8 mask, per set bit: u32 byteLen + UTF-16LE (NUL-terminated)`.
fn property_block(block: &[u8]) -> Result<(u8, Vec<String>)> {
    let declared = u32_at(block, 0)? as usize;
    ensure!(declared + 4 == block.len(), "property block length {declared} + 4 != {}", block.len());
    let mask = *block.get(4).context("empty property block")?;
    let mut pos = 5;
    let mut props = Vec::new();
    for bit in (0..8).rev() {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let len = u32_at(block, pos)? as usize;
        pos += 4;
        let raw = block.get(pos..pos + len).context("property string out of range")?;
        pos += len;
        ensure!(len >= 2 && len.is_multiple_of(2) && raw[len - 2..] == [0, 0], "property string not NUL-terminated UTF-16");
        let units: Vec<u16> = raw[..len - 2].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        props.push(String::from_utf16(&units).context("invalid UTF-16 in property")?);
    }
    ensure!(pos == block.len(), "{} trailing bytes in property block", block.len() - pos);
    Ok((mask, props))
}

/// Scans every leaf entry of a `$WW$CTCATALOG` B-tree, in key order.
///
/// Checks: leaf chain integrity, entries filling each page exactly, ascending keys, DATA ranges
/// contiguous and ending at the end of DATA, and property-block lengths.
pub fn scan(btree: &[u8], data: &[u8]) -> Result<Vec<CatalogEntry>> {
    let geo = geometry(btree)?;
    let page = |n: u64| -> Result<&[u8]> {
        ensure!((n as usize) < geo.page_count, "page {n} out of range");
        let start = geo.first_page + n as usize * geo.page_size;
        Ok(&btree[start..start + geo.page_size])
    };

    // The chain starts at the leaf without a predecessor.
    let mut head = None;
    for n in 0..geo.page_count as u64 {
        let p = page(n)?;
        if &p[..4] == LEAF_MAGIC && u64_at(p, 0x10)? == NO_PAGE {
            ensure!(head.is_none(), "more than one leaf without a predecessor");
            head = Some(n);
        } else {
            ensure!(&p[..4] == LEAF_MAGIC || &p[..4] == INDEX_MAGIC, "page {n} has unknown magic");
        }
    }
    let mut current = head.context("no first leaf page")?;

    let mut entries: Vec<CatalogEntry> = Vec::new();
    let mut data_cursor = 0usize;
    let mut prev_page = NO_PAGE;
    let mut visited = 0usize;
    loop {
        visited += 1;
        ensure!(visited <= geo.page_count, "leaf chain loops");
        let p = page(current)?;
        ensure!(&p[..4] == LEAF_MAGIC, "chain reaches non-leaf page {current}");
        let unused = u32_at(p, 4)? as usize;
        ensure!(u64_at(p, 8)? == current, "page {current} has wrong self number");
        ensure!(u64_at(p, 0x10)? == prev_page, "page {current} has wrong back link");
        ensure!(u64_at(p, 0x20)? == entries.len() as u64, "page {current} first-entry ordinal mismatch");
        let next = u64_at(p, 0x18)?;
        let end = geo.page_size.checked_sub(unused).context("unused bytes exceed page")?;
        ensure!(end >= LEAF_HEADER_LEN, "page {current} entry area underflows");

        let mut pos = LEAF_HEADER_LEN;
        while pos < end {
            let key = u32_at(p, pos)?;
            pos += 4;
            let len = varint_le(p, &mut pos)? as usize;
            let body = p.get(pos..pos + len).context("entry runs past page")?;
            pos += len;
            ensure!(pos <= end, "entry runs into the page's unused area");
            if let Some(last) = entries.last() {
                ensure!(key > last.key, "keys not ascending at {key}");
            }
            entries.push(decode_entry(key, body, data, &mut data_cursor)?);
        }
        ensure!(pos == end, "page {current} entry area overruns");

        if next == NO_PAGE {
            break;
        }
        prev_page = current;
        current = next;
    }
    ensure!(data_cursor == data.len(), "DATA has {} unowned trailing bytes", data.len() - data_cursor);
    Ok(entries)
}

fn decode_entry(key: u32, body: &[u8], data: &[u8], data_cursor: &mut usize) -> Result<CatalogEntry> {
    ensure!(body.len() >= 2 && body[0] == 1, "entry {key}: unexpected lead byte");
    let inline = match body[1] {
        0 => false,
        1 => true,
        x => bail!("entry {key}: inline flag {x}"),
    };
    let mut pos = 2;
    let inline_len = encint(body, &mut pos)? as usize;
    let data_len = encint(body, &mut pos)? as usize;
    let data_off = encint(body, &mut pos)? as usize;
    ensure!(data_off == *data_cursor, "entry {key}: DATA offset {data_off} != expected {}", *data_cursor);
    let range = data.get(data_off..data_off + data_len).with_context(|| format!("entry {key}: DATA range out of bounds"))?;
    *data_cursor = data_off + data_len;
    ensure!(range.len() >= 4 && range[..4] == [0, 0, 0, 0], "entry {key}: DATA range lacks its zero word");
    ensure!(body.len() - pos == inline_len, "entry {key}: inline length mismatch");
    let block = if inline {
        ensure!(data_len == 4, "entry {key}: inline entry owns {data_len} DATA bytes");
        &body[pos..]
    } else {
        ensure!(inline_len == 0, "entry {key}: external entry with inline bytes");
        &range[4..]
    };
    let (present_mask, properties) = property_block(block).with_context(|| format!("entry {key}"))?;
    Ok(CatalogEntry { key, present_mask, properties, inline })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16z(s: &str) -> Vec<u8> {
        let mut v: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        v.extend([0, 0]);
        v
    }

    fn block(props: &[&str], mask: u8) -> Vec<u8> {
        let mut inner = vec![mask];
        for p in props {
            let s = utf16z(p);
            inner.extend((s.len() as u32).to_le_bytes());
            inner.extend(s);
        }
        let mut b = (inner.len() as u32).to_le_bytes().to_vec();
        b.extend(inner);
        b
    }

    fn encint_bytes(mut v: u64) -> Vec<u8> {
        let mut out = vec![(v & 0x7f) as u8];
        v >>= 7;
        while v > 0 {
            out.push(0x80 | (v & 0x7f) as u8);
            v >>= 7;
        }
        out.reverse();
        out
    }

    fn varint_bytes(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// Builds a BTREE with one index page and two leaves, and the matching DATA stream.
    fn synthetic(entries: &[(u32, Vec<&str>, bool)], split: usize) -> (Vec<u8>, Vec<u8>) {
        const PS: usize = 0x200;
        const FIRST: usize = 0x1dc;
        let mut data = Vec::new();
        let mut encoded = Vec::new();
        for (key, props, inline) in entries {
            let blk = block(props, if props.len() == 2 { 0xc0 } else { 0x80 });
            let off = data.len() as u64;
            let mut body = vec![1u8, *inline as u8];
            data.extend([0, 0, 0, 0]);
            if *inline {
                body.extend(encint_bytes(blk.len() as u64));
                body.extend(encint_bytes(4));
                body.extend(encint_bytes(off));
                body.extend(&blk);
            } else {
                data.extend(&blk);
                body.extend(encint_bytes(0));
                body.extend(encint_bytes(4 + blk.len() as u64));
                body.extend(encint_bytes(off));
            }
            let mut e = key.to_le_bytes().to_vec();
            e.extend(varint_bytes(body.len() as u64));
            e.extend(body);
            encoded.push(e);
        }
        let pages = 3usize;
        let mut b = vec![0u8; FIRST + pages * PS];
        b[..8].copy_from_slice(b"ITOLITLS");
        b[0x48..0x50].copy_from_slice(&((FIRST + pages * PS) as u64).to_le_bytes());
        b[0x124..0x128].copy_from_slice(&(PS as u32).to_le_bytes());
        b[0x144..0x14c].copy_from_slice(&(FIRST as u64).to_le_bytes());
        // page 0 and 2 are leaves; page 1 is an index page
        let leaf = |b: &mut Vec<u8>, n: usize, prev: u64, next: u64, first: u64, es: &[Vec<u8>]| {
            let base = FIRST + n * PS;
            let mut pos = base + LEAF_HEADER_LEN;
            for e in es {
                b[pos..pos + e.len()].copy_from_slice(e);
                pos += e.len();
            }
            b[base..base + 4].copy_from_slice(LEAF_MAGIC);
            b[base + 4..base + 8].copy_from_slice(&((base + PS - pos) as u32).to_le_bytes());
            b[base + 8..base + 16].copy_from_slice(&(n as u64).to_le_bytes());
            b[base + 16..base + 24].copy_from_slice(&prev.to_le_bytes());
            b[base + 24..base + 32].copy_from_slice(&next.to_le_bytes());
            b[base + 32..base + 40].copy_from_slice(&first.to_le_bytes());
        };
        leaf(&mut b, 0, NO_PAGE, 2, 0, &encoded[..split]);
        b[FIRST + PS..FIRST + PS + 4].copy_from_slice(INDEX_MAGIC);
        leaf(&mut b, 2, 0, NO_PAGE, split as u64, &encoded[split..]);
        (b, data)
    }

    #[test]
    fn scans_inline_and_external_entries() {
        let long = "x".repeat(150);
        let entries = vec![
            (10, vec!["i;L1;B1.jpg;P1.JSM;U0;CSomeone", "Title one"], true),
            (20, vec![long.as_str(), "Long one"], false),
            (30, vec!["Only title"], true),
        ];
        let (btree, data) = synthetic(&entries, 2);
        let out = scan(&btree, &data).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].key, 10);
        assert_eq!(out[0].properties, vec!["i;L1;B1.jpg;P1.JSM;U0;CSomeone", "Title one"]);
        assert!(out[0].inline);
        assert!(!out[1].inline);
        assert_eq!(out[1].properties[0], long);
        assert_eq!(out[2].present_mask, 0x80);
        assert_eq!(out[2].properties, vec!["Only title"]);
    }

    #[test]
    fn rejects_unsorted_keys_and_leftover_data() {
        let entries = vec![(20, vec!["a"], true), (10, vec!["b"], true)];
        let (btree, data) = synthetic(&entries, 1);
        assert!(scan(&btree, &data).is_err());

        let entries = vec![(1, vec!["a"], true)];
        let (btree, mut data) = synthetic(&entries, 1);
        data.push(0);
        assert!(scan(&btree, &data).is_err());
    }

    #[test]
    fn integer_codings() {
        let mut pos = 0;
        assert_eq!(encint(&[0x81, 0x59], &mut pos).unwrap(), 217);
        pos = 0;
        assert_eq!(varint_le(&[0xdf, 0x01], &mut pos).unwrap(), 223);
    }
}
