//! Huffman decode tables stored in decoding form (mad.md §3).

use super::bits::BitReader;
use anyhow::{Result, bail, ensure};

#[derive(Debug, Clone, Copy)]
struct Step {
    limit: u32,
    base: i32,
    len: u32,
}

/// One canonical decode table: `(limit, base, len)` steps in increasing `limit` order (the anchor
/// first, the 0xFFFFFFFF sentinel last) plus the symbol array.
#[derive(Debug, Clone)]
pub struct HuffTable {
    steps: Vec<Step>,
    symbols: Vec<u16>,
    /// For each top byte of the 32-bit window, the first step whose limit can cover it.
    start: Box<[u8; 256]>,
}

/// Width of the stored symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolWidth {
    U8,
    U16,
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    match b.get(o..o + 4) {
        Some(s) => Ok(u32::from_le_bytes(s.try_into().unwrap())),
        None => bail!("Huffman table runs past its buffer (offset {o})"),
    }
}

impl HuffTable {
    /// Reads the table whose anchor triplet is at byte `anchor` of `buf`. Steps are read from the
    /// anchor towards lower addresses until the sentinel (limit 0xFFFFFFFF).
    pub fn parse(buf: &[u8], anchor: usize, width: SymbolWidth) -> Result<HuffTable> {
        let mut steps = Vec::new();
        let mut q = anchor as isize;
        loop {
            ensure!(q >= 0, "Huffman table at {anchor}: no sentinel below the anchor");
            let o = q as usize;
            let step = Step { limit: u32_at(buf, o)?, base: u32_at(buf, o + 4)? as i32, len: u32_at(buf, o + 8)? };
            ensure!(step.len <= 32, "Huffman table at {anchor}: code length {}", step.len);
            if let Some(prev) = steps.last() {
                let prev: &Step = prev;
                ensure!(step.limit >= prev.limit, "Huffman table at {anchor}: limits not increasing");
            }
            steps.push(step);
            if step.limit == u32::MAX {
                break;
            }
            ensure!(steps.len() <= 64, "Huffman table at {anchor}: too many code lengths");
            q -= 12;
        }
        let count = (u32_at(buf, anchor + 12)? & 0xFFFF) as usize;
        let sym_at = anchor + 16;
        let symbols: Vec<u16> = match width {
            SymbolWidth::U8 => {
                let s = buf.get(sym_at..sym_at + count);
                let Some(s) = s else { bail!("Huffman table at {anchor}: symbols truncated") };
                s.iter().map(|&b| b as u16).collect()
            }
            SymbolWidth::U16 => {
                let s = buf.get(sym_at..sym_at + 2 * count);
                let Some(s) = s else { bail!("Huffman table at {anchor}: symbols truncated") };
                s.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
            }
        };
        Ok(Self::from_parts(steps, symbols))
    }

    fn from_parts(steps: Vec<Step>, symbols: Vec<u16>) -> HuffTable {
        let mut start = Box::new([0u8; 256]);
        let mut i = 0usize;
        for (top, slot) in start.iter_mut().enumerate() {
            let lowest = (top as u32) << 24;
            while i + 1 < steps.len() && steps[i].limit < lowest {
                i += 1;
            }
            *slot = i as u8;
        }
        HuffTable { steps, symbols, start }
    }

    /// Builds a table from code-length counts, for tests: `counts[l]` symbols get `l`-bit codes, in
    /// symbol order (canonical, shorter codes first).
    #[cfg(test)]
    pub fn canonical(counts: &[(u32, usize)], symbols: Vec<u16>) -> HuffTable {
        let mut steps = Vec::new();
        let mut code: u64 = 0; // next code, left-justified in 32 bits
        let mut index = 0i64;
        for &(len, n) in counts {
            if n == 0 {
                continue;
            }
            let first = code >> (32 - len);
            code += (n as u64) << (32 - len);
            let limit = (code - 1) as u32;
            steps.push(Step { limit, base: (index - first as i64) as i32, len });
            index += n as i64;
        }
        steps.last_mut().unwrap().limit = u32::MAX;
        Self::from_parts(steps, symbols)
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    /// Decodes one symbol.
    #[inline]
    pub fn decode(&self, r: &mut BitReader) -> Result<u16> {
        let w = r.peek32();
        let mut i = self.start[(w >> 24) as usize] as usize;
        while self.steps[i].limit < w {
            i += 1;
        }
        let s = self.steps[i];
        let code = if s.len == 0 { 0 } else { (w >> (32 - s.len)) as i64 };
        let idx = code + s.base as i64;
        r.skip(s.len);
        match self.symbols.get(idx as usize) {
            Some(&v) if idx >= 0 => Ok(v),
            _ => bail!("Huffman symbol index {idx} out of range ({} symbols)", self.symbols.len()),
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Serialises a table into the stored layout: triplets lowest address first (sentinel first),
    /// anchor triplet last, then `nsym` and the symbols. Returns the bytes and the anchor offset.
    pub fn blob(steps_low_to_high_addr: &[(u32, i32, u32)], symbols: &[u16], wide: bool) -> (Vec<u8>, usize) {
        let mut b = Vec::new();
        for &(l, base, len) in steps_low_to_high_addr {
            b.extend(l.to_le_bytes());
            b.extend(base.to_le_bytes());
            b.extend(len.to_le_bytes());
        }
        let anchor = b.len() - 12;
        b.extend((symbols.len() as u32).to_le_bytes());
        for &s in symbols {
            if wide { b.extend(s.to_le_bytes()) } else { b.push(s as u8) }
        }
        (b, anchor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mad::bits::testutil::BitWriter;

    #[test]
    fn decodes_stored_table() {
        // Codes: 0 → 'a', 10 → 'b', 110 → 'c', 111 → 'd' (lowest address = sentinel).
        let (blob, anchor) = testutil::blob(
            &[(0xFFFF_FFFF, -4, 3), (0xBFFF_FFFF, -1, 2), (0x7FFF_FFFF, 0, 1)],
            &[b'a' as u16, b'b' as u16, b'c' as u16, b'd' as u16],
            false,
        );
        let t = HuffTable::parse(&blob, anchor, SymbolWidth::U8).unwrap();
        let mut w = BitWriter::default();
        for (code, len) in [(0b110, 3), (0, 1), (0b111, 3), (0b10, 2), (0, 1)] {
            w.put(code, len);
        }
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        let got: Vec<u8> = (0..5).map(|_| t.decode(&mut r).unwrap() as u8).collect();
        assert_eq!(&got, b"cadba");
        assert_eq!(r.position(), 10);
    }

    #[test]
    fn canonical_builder_matches_stored_form() {
        let t = HuffTable::canonical(&[(1, 1), (2, 1), (3, 2)], vec![7, 8, 9, 10]);
        let mut w = BitWriter::default();
        w.put(0b111, 3);
        w.put(0b10, 2);
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert_eq!(t.decode(&mut r).unwrap(), 10);
        assert_eq!(t.decode(&mut r).unwrap(), 8);
    }

    #[test]
    fn rejects_missing_sentinel_and_bad_index() {
        let (blob, anchor) = testutil::blob(&[(0x7FFF_FFFF, 0, 1)], &[1], false);
        assert!(HuffTable::parse(&blob, anchor, SymbolWidth::U8).is_err());
        let (blob, anchor) = testutil::blob(&[(0xFFFF_FFFF, 5, 1)], &[1, 2], true);
        let t = HuffTable::parse(&blob, anchor, SymbolWidth::U16).unwrap();
        let mut r = BitReader::new(&[0, 0, 0, 0]);
        assert!(t.decode(&mut r).is_err());
    }
}
