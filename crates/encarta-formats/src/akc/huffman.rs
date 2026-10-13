//! Canonical Huffman codes and their serialized code lengths (spec §5).

use super::bits::BitReader;
use anyhow::{Result, bail, ensure};

const MAX_LEN: usize = 31;
const MAX_SYMBOLS: usize = 2048;
/// Width of the direct lookup table.
const TABLE_BITS: u32 = 10;

#[derive(Debug, Clone)]
enum Shape {
    /// No symbol used: valid, but must never be decoded from.
    Empty,
    /// One symbol used: decoding it consumes no bits.
    Single(u16),
    Multi,
}

/// A canonical prefix code (same convention as Deflate) decoded MSB first.
#[derive(Debug, Clone)]
pub struct Huffman {
    shape: Shape,
    table_bits: u32,
    /// `symbol << 8 | length` for codes of at most `table_bits`; 0 for longer prefixes.
    table: Vec<u32>,
    /// Canonical decoding for long codes, per length.
    first_code: [u32; MAX_LEN + 1],
    count: [u32; MAX_LEN + 1],
    first_index: [u32; MAX_LEN + 1],
    sorted: Vec<u16>,
    max_len: u32,
}

impl Huffman {
    /// Builds the code from per-symbol lengths (0 = unused), checking §5.1's validity rules.
    #[allow(clippy::needless_range_loop)] // parallel per-length arrays
    pub fn new(lengths: &[u8]) -> Result<Self> {
        ensure!(
            lengths.len() <= MAX_SYMBOLS,
            "code has {} symbols",
            lengths.len()
        );
        let mut count = [0u32; MAX_LEN + 1];
        for &l in lengths {
            ensure!((l as usize) <= MAX_LEN, "code length {l} > {MAX_LEN}");
            count[l as usize] += 1;
        }
        count[0] = 0;
        let used: u32 = count.iter().sum();
        let mut h = Huffman {
            shape: Shape::Multi,
            table_bits: 0,
            table: Vec::new(),
            first_code: [0; MAX_LEN + 1],
            count,
            first_index: [0; MAX_LEN + 1],
            sorted: Vec::new(),
            max_len: 0,
        };
        if used == 0 {
            h.shape = Shape::Empty;
            return Ok(h);
        }
        if used == 1 {
            let s = lengths.iter().position(|&l| l != 0).unwrap();
            ensure!(
                lengths[s] == 1,
                "single-symbol code with length {}",
                lengths[s]
            );
            h.shape = Shape::Single(s as u16);
            return Ok(h);
        }
        // Kraft sum must be exactly 1.
        let kraft: u64 = (1..=MAX_LEN)
            .map(|l| (count[l] as u64) << (MAX_LEN - l))
            .sum();
        ensure!(
            kraft == 1u64 << MAX_LEN,
            "incomplete or over-subscribed code (Kraft {kraft:#x})"
        );
        h.max_len = (1..=MAX_LEN).rev().find(|&l| count[l] > 0).unwrap() as u32;

        // Canonical first codes and symbol order.
        let mut code = 0u32;
        let mut index = 0u32;
        for l in 1..=MAX_LEN {
            h.first_code[l] = code;
            h.first_index[l] = index;
            index += count[l];
            code = (code + count[l]) << 1;
        }
        let mut next = h.first_index;
        h.sorted = vec![0; used as usize];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                h.sorted[next[l as usize] as usize] = s as u16;
                next[l as usize] += 1;
            }
        }

        // Direct lookup table.
        let tb = h.max_len.min(TABLE_BITS);
        h.table_bits = tb;
        h.table = vec![0; 1 << tb];
        for l in 1..=tb as usize {
            for i in 0..count[l] {
                let sym = h.sorted[(h.first_index[l] + i) as usize] as u32;
                let c = h.first_code[l] + i;
                let lo = (c << (tb as usize - l)) as usize;
                let hi = ((c + 1) << (tb as usize - l)) as usize;
                h.table[lo..hi].fill(sym << 8 | l as u32);
            }
        }
        Ok(h)
    }

    /// Decodes one symbol.
    #[inline]
    pub fn decode(&self, r: &mut BitReader) -> Result<u32> {
        match self.shape {
            Shape::Multi => {}
            Shape::Single(s) => return Ok(s as u32),
            Shape::Empty => bail!("decoding from an empty code"),
        }
        let bits = r.peek32();
        let e = self.table[(bits >> (32 - self.table_bits)) as usize];
        if e != 0 {
            r.consume(e & 0xFF);
            return Ok(e >> 8);
        }
        for l in self.table_bits + 1..=self.max_len {
            let c = bits >> (32 - l);
            let off = c.wrapping_sub(self.first_code[l as usize]);
            if off < self.count[l as usize] {
                r.consume(l);
                return Ok(self.sorted[(self.first_index[l as usize] + off) as usize] as u32);
            }
        }
        unreachable!("complete code always matches")
    }
}

/// Number of bits needed to write `v` (`bitlength(0) = 0`).
fn bit_length(v: u32) -> u32 {
    32 - v.leading_zeros()
}

/// Which serialization a code used (for statistics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthsMode {
    PreTree,
    Sparse,
    Flat,
}

/// Reads `n` serialized code lengths (§5.2).
pub fn read_lengths_mode(r: &mut BitReader, n: usize) -> Result<(Vec<u8>, LengthsMode)> {
    let mut len = vec![0u8; n];
    if r.bit() {
        // Pre-tree mode.
        let a = r.read(6) as usize;
        if a == 0 {
            return Ok((len, LengthsMode::PreTree));
        }
        ensure!(a <= 32, "pre-tree literal count {a} > 32");
        let b = r.read(4) as usize;
        ensure!(b < 12, "pre-tree repeat count {b} >= 12");
        let mut pre = [0u8; 44];
        for l in pre.iter_mut().take(a) {
            *l = r.read(3) as u8;
        }
        for l in pre[32..32 + b].iter_mut() {
            *l = r.read(3) as u8;
        }
        let pre = Huffman::new(&pre)?;
        let mut i = 0;
        while i < n {
            r.check("pre-tree lengths")?;
            let s = pre.decode(r)?;
            if s < 32 {
                len[i] = s as u8;
                i += 1;
            } else {
                let k = s - 32;
                ensure!(i > 0, "pre-tree repeat with no previous length");
                let rep = (1usize << k) + r.read(k) as usize;
                ensure!(i + rep <= n, "pre-tree repeat overshoots {n} lengths");
                let prev = len[i - 1];
                len[i..i + rep].fill(prev);
                i += rep;
            }
        }
        Ok((len, LengthsMode::PreTree))
    } else if r.bit() {
        // Sparse mode.
        let bw = bit_length(n as u32 - 1);
        let count = r.read(bw) as usize + 1;
        ensure!(count <= n, "sparse code lists {count} of {n} symbols");
        let mut l = r.read(5);
        let mut prev: Option<u32> = None;
        let mut assigned = 0;
        while assigned < count {
            r.check("sparse lengths")?;
            let s = r.read(bw);
            ensure!((s as usize) < n, "sparse symbol {s} >= {n}");
            if prev == Some(s) {
                l += 1;
                ensure!(l <= 31, "sparse code length exceeds 31");
            } else {
                len[s as usize] = l as u8;
                assigned += 1;
            }
            prev = Some(s);
        }
        Ok((len, LengthsMode::Sparse))
    } else {
        // Flat mode.
        let w = r.read(3);
        ensure!(w <= 5, "flat code-length width {w} > 5");
        for l in len.iter_mut() {
            let v = r.read(w);
            ensure!(v <= 31, "flat code length {v} > 31");
            *l = v as u8;
        }
        Ok((len, LengthsMode::Flat))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::bits::tests::BitWriter;
    use super::*;

    /// Canonical code of each symbol, as (code, length).
    pub fn canonical(lengths: &[u8]) -> Vec<(u32, u32)> {
        let mut syms: Vec<usize> = (0..lengths.len()).filter(|&s| lengths[s] > 0).collect();
        syms.sort_by_key(|&s| (lengths[s], s));
        let mut out = vec![(0, 0); lengths.len()];
        let mut code = 0u32;
        let mut prev_len = 0;
        for (i, &s) in syms.iter().enumerate() {
            let l = lengths[s] as u32;
            if i > 0 {
                code = (code + 1) << (l - prev_len);
            } else {
                code <<= l;
            }
            prev_len = l;
            out[s] = (code, l);
        }
        out
    }

    #[test]
    fn spec_pretree_example() {
        // §10.2 pre-tree: symbols 0..10 then 32..37.
        let mut lens = [0u8; 44];
        lens[..11].copy_from_slice(&[3, 0, 0, 6, 5, 5, 4, 4, 3, 3, 3]);
        lens[32..38].copy_from_slice(&[4, 4, 5, 3, 0, 6]);
        let c = canonical(&lens);
        assert_eq!(c[0], (0b000, 3));
        assert_eq!(c[35], (0b100, 3));
        assert_eq!(c[6], (0b1010, 4));
        assert_eq!(c[3], (0b111110, 6));
        assert_eq!(c[37], (0b111111, 6));
        let h = Huffman::new(&lens).unwrap();
        let mut w = BitWriter::default();
        for s in [6usize, 32, 7, 8, 35, 37, 3, 0] {
            w.put(c[s].1, c[s].0 as u64);
        }
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        for s in [6u32, 32, 7, 8, 35, 37, 3, 0] {
            assert_eq!(h.decode(&mut r).unwrap(), s);
        }
    }

    #[test]
    fn long_codes_and_shapes() {
        // Lengths 1, 2, …, 30, 30: complete, with codes beyond the lookup table.
        let mut lens: Vec<u8> = (1..=30).collect();
        lens.push(30);
        let h = Huffman::new(&lens).unwrap();
        let c = canonical(&lens);
        let mut w = BitWriter::default();
        let order = [30usize, 0, 29, 12, 9, 10, 1];
        for &s in &order {
            w.put(c[s].1, c[s].0 as u64);
        }
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        for &s in &order {
            assert_eq!(h.decode(&mut r).unwrap(), s as u32);
        }
        assert_eq!(r.pos(), w.len() as u64);

        // Single symbol: zero bits.
        let h = Huffman::new(&[0, 0, 1]).unwrap();
        let mut r = BitReader::new(&[0xff, 0xff]).unwrap();
        assert_eq!(h.decode(&mut r).unwrap(), 2);
        assert_eq!(r.pos(), 0);
        assert!(Huffman::new(&[0, 2]).is_err());
        // Empty: accepted, never decodable.
        assert!(Huffman::new(&[0, 0]).unwrap().decode(&mut r).is_err());
        // Incomplete / over-subscribed.
        assert!(Huffman::new(&[1, 2]).is_err());
        assert!(Huffman::new(&[1, 1, 1]).is_err());
    }

    #[test]
    fn length_modes() {
        // Pre-tree: literal lengths 1 and 2, repeat symbol 32 (k = 0, ×1).
        // Pre-tree code: symbols 1, 2, 32 with lengths 1, 2, 2 → 1=0, 2=10, 32=11.
        let mut w = BitWriter::default();
        w.put(1, 1).put(6, 3).put(4, 1);
        w.put(3, 0).put(3, 1).put(3, 2); // symbols 0, 1, 2
        w.put(3, 2); // symbol 32
        // lengths: 1, 2, (repeat 2) → [1, 2, 2]
        w.put(1, 0).put(2, 0b10).put(2, 0b11);
        // Sparse: n = 4 (bw = 2), 3 symbols, L starts at 1: s=2 (L1), s=0 → L1, s=0 again bumps L → 2, s=3 (L2).
        w.put(1, 0).put(1, 1).put(2, 2).put(5, 1);
        w.put(2, 2).put(2, 0).put(2, 0).put(2, 3);
        // Flat: w = 2, lengths 1 2 3 3.
        w.put(1, 0).put(1, 0).put(3, 2);
        for v in [1, 2, 3, 3] {
            w.put(2, v);
        }
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        let (l, m) = read_lengths_mode(&mut r, 3).unwrap();
        assert_eq!((l, m), (vec![1, 2, 2], LengthsMode::PreTree));
        let (l, m) = read_lengths_mode(&mut r, 4).unwrap();
        // s=2 → len[2]=1; s=0 → len[0]=1; repeat 0 → L=2; s=3 → len[3]=2.
        assert_eq!((l, m), (vec![1, 0, 1, 2], LengthsMode::Sparse));
        let (l, m) = read_lengths_mode(&mut r, 4).unwrap();
        assert_eq!((l, m), (vec![1, 2, 3, 3], LengthsMode::Flat));
        assert_eq!(r.pos(), w.len() as u64);
    }
}
