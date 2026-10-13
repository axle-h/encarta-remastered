//! The token stream of a compressed block (spec §7).

use super::bits::BitReader;
use super::dict::{CLASS_FIRST, class_extra};
use super::huffman::Huffman;
use anyhow::{Result, bail};

/// Flag marking a token that refers to the shared dictionary; the low bits are the word number.
pub const SHARED: u32 = 1 << 31;

/// Statistics of one decoded stream.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenStats {
    pub literals: u64,
    pub matches: u64,
}

/// Match length (before the +2) for length code `j` (§7).
#[inline]
fn match_base(r: &mut BitReader, j: u32) -> u32 {
    match j {
        0..=7 => j,
        8..=11 => 2 * j - 8 + r.read(1),
        12..=14 => {
            let k = j - 10;
            (1 << k) + 12 + r.read(k)
        }
        _ => {
            let k = r.read(4);
            (1 << k) + 43 + r.read(k)
        }
    }
}

/// Decodes symbols until `need` tokens exist. A token is a block word number,
/// or a shared word number with [`SHARED`] set.
pub fn decode_tokens(
    r: &mut BitReader,
    code: &Huffman,
    block_words: u32,
    shared_words: u32,
    need: usize,
) -> Result<(Vec<u32>, TokenStats)> {
    let mut t: Vec<u32> = Vec::with_capacity(need.min(1 << 22));
    let mut st = TokenStats::default();
    while t.len() < need {
        if r.overrun() {
            bail!("token stream: read past end after {} tokens", t.len());
        }
        let s = code.decode(r)?;
        if s < 64 {
            let c = s & 31;
            let v = CLASS_FIRST[c as usize] + r.read(class_extra(c));
            if s < 32 {
                if v >= block_words {
                    bail!("token {}: block word {v} >= {block_words}", t.len());
                }
                t.push(v);
            } else {
                if v >= shared_words {
                    bail!("token {}: shared word {v} >= {shared_words}", t.len());
                }
                t.push(v | SHARED);
            }
            st.literals += 1;
        } else {
            let m = s - 64;
            let len = match_base(r, m >> 4) as usize + 2;
            let d = m & 15;
            let dist = ((1u32 << d) + r.read(d)) as usize;
            let have = t.len();
            if dist > have {
                bail!("token {have}: match distance {dist} before block start");
            }
            let n = len.min(need - have);
            let from = have - dist;
            if dist >= n {
                t.extend_from_within(from..from + n);
            } else {
                for i in 0..n {
                    let v = t[from + i];
                    t.push(v);
                }
            }
            st.matches += 1;
        }
    }
    if r.overrun() {
        bail!("token stream: read past end");
    }
    Ok((t, st))
}

#[cfg(test)]
mod tests {
    use super::super::bits::tests::BitWriter;
    use super::*;

    #[test]
    fn literals_and_overlapping_match() {
        // Flat 9-bit code over 320 symbols is impossible (incomplete), so build a
        // complete code: symbols 0, 33, 64+16*0+1 (len 2, d = 1), 64+16*15+0 (j = 15).
        let mut lens = vec![0u8; 320];
        lens[0] = 2;
        lens[33] = 2;
        lens[65] = 2;
        lens[304] = 2;
        let code = Huffman::new(&lens).unwrap();
        // Canonical: 0=00, 33=01, 65=10, 304=11.
        let mut w = BitWriter::default();
        w.put(2, 0b00); // block word 0
        w.put(2, 0b01); // shared class 1 → word 1
        w.put(2, 0b10).put(1, 1); // match len 2, distance 2 + 1 = 3 > 2 tokens → error case later
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        assert!(decode_tokens(&mut r, &code, 4, 4, 4).is_err());

        let mut w = BitWriter::default();
        w.put(2, 0b00);
        w.put(2, 0b01);
        w.put(2, 0b10).put(1, 0); // len 2, dist 2 → copy [0, S|1]
        // j = 15, d = 0: k = read(4) = 0 → base 1 + 43 + 0 = 44, len 46, dist 1.
        w.put(2, 0b11).put(4, 0);
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        let (t, st) = decode_tokens(&mut r, &code, 4, 4, 20).unwrap();
        assert_eq!(&t[..4], &[0, SHARED | 1, 0, SHARED | 1]);
        assert!(t[4..].iter().all(|&v| v == SHARED | 1));
        assert_eq!(t.len(), 20);
        assert_eq!((st.literals, st.matches), (2, 2));
    }

    #[test]
    fn match_lengths() {
        let bits = |f: &dyn Fn(&mut BitWriter)| {
            let mut w = BitWriter::default();
            f(&mut w);
            w.bytes()
        };
        let cases: [(u32, Vec<u8>, u32); 6] = [
            (3, bits(&|_| {}), 3),
            (
                8,
                bits(&|w| {
                    w.put(1, 1);
                }),
                9,
            ),
            (
                11,
                bits(&|w| {
                    w.put(1, 1);
                }),
                15,
            ),
            (
                12,
                bits(&|w| {
                    w.put(2, 3);
                }),
                19,
            ),
            (
                14,
                bits(&|w| {
                    w.put(4, 15);
                }),
                43,
            ),
            (
                15,
                bits(&|w| {
                    w.put(4, 15).put(15, 0x7FFF);
                }),
                65_578,
            ),
        ];
        for (j, data, want) in cases {
            let mut data = data;
            data.resize(8, 0);
            let mut r = BitReader::new(&data).unwrap();
            assert_eq!(match_base(&mut r, j), want, "j = {j}");
        }
    }
}
