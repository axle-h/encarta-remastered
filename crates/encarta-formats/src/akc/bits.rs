//! Bit reader for AKC bit streams (spec §1): the stream is a sequence of 16-bit
//! little-endian words, each consumed most-significant bit first.

use anyhow::{Result, bail};

/// Reads MSB-first fields from a stream of 16-bit little-endian words.
///
/// Reads past the end yield zero bits; callers check [`BitReader::overrun`] at
/// natural boundaries (end of a dictionary, each token) and treat it as corruption.
#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Index of the next 16-bit word to load into `buf`.
    next_word: usize,
    /// Pending bits, left-aligned (bit 63 is the next bit).
    buf: u64,
    /// Number of valid bits in `buf`.
    avail: u32,
}

impl<'a> BitReader<'a> {
    /// A reader positioned at bit 0. The stream must have an even byte length.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if !data.len().is_multiple_of(2) {
            bail!("bit stream has odd length {}", data.len());
        }
        let mut r = BitReader {
            data,
            next_word: 0,
            buf: 0,
            avail: 0,
        };
        r.refill();
        Ok(r)
    }

    /// A reader positioned at bit `bit`.
    pub fn at(data: &'a [u8], bit: u64) -> Result<Self> {
        let mut r = Self::new(data)?;
        r.seek(bit)?;
        Ok(r)
    }

    /// Total length of the stream in bits.
    pub fn bit_len(&self) -> u64 {
        self.data.len() as u64 * 8
    }

    /// Current position in bits from the start of the stream.
    pub fn pos(&self) -> u64 {
        self.next_word as u64 * 16 - self.avail as u64
    }

    /// True once more bits have been consumed than the stream holds.
    pub fn overrun(&self) -> bool {
        self.pos() > self.bit_len()
    }

    /// Fails if the reader has consumed bits past the end of the stream.
    pub fn check(&self, what: &str) -> Result<()> {
        if self.overrun() {
            bail!(
                "{what}: read past end of bit stream ({} > {} bits)",
                self.pos(),
                self.bit_len()
            );
        }
        Ok(())
    }

    pub fn seek(&mut self, bit: u64) -> Result<()> {
        if bit > self.bit_len() {
            bail!("seek to bit {bit} beyond stream of {} bits", self.bit_len());
        }
        self.next_word = (bit >> 4) as usize;
        self.buf = 0;
        self.avail = 0;
        self.refill();
        self.consume((bit & 15) as u32);
        Ok(())
    }

    #[inline]
    fn load_word(&self, i: usize) -> u64 {
        match self.data.get(2 * i..2 * i + 2) {
            Some(b) => u16::from_le_bytes([b[0], b[1]]) as u64,
            None => 0,
        }
    }

    /// Tops `buf` up to at least 49 valid bits.
    #[inline]
    fn refill(&mut self) {
        while self.avail <= 48 {
            let w = self.load_word(self.next_word);
            self.buf |= w << (48 - self.avail);
            self.avail += 16;
            self.next_word += 1;
        }
    }

    /// The next 32 bits without consuming them (zero bits past the end).
    #[inline]
    pub fn peek32(&mut self) -> u32 {
        if self.avail < 32 {
            self.refill();
        }
        (self.buf >> 32) as u32
    }

    /// Consumes `n ≤ 32` bits that were peeked.
    #[inline]
    pub fn consume(&mut self, n: u32) {
        debug_assert!(n <= self.avail);
        if n == 0 {
            return;
        }
        self.buf <<= n;
        self.avail -= n;
    }

    /// Reads an `n`-bit field (`n ≤ 32`), MSB first. `read(0)` is 0.
    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        if self.avail < n {
            self.refill();
        }
        let v = (self.buf >> (64 - n)) as u32;
        self.buf <<= n;
        self.avail -= n;
        v
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.read(1) == 1
    }

    /// The variable-length integer `num()` (§1): `w = read(5)`; 0 if `w == 0`,
    /// else `(2^w − 1) + read(w)`.
    pub fn num(&mut self) -> u64 {
        let w = self.read(5);
        if w == 0 {
            0
        } else {
            ((1u64 << w) - 1) + self.read(w) as u64
        }
    }

    /// Remaining bits up to the end of the stream (0 if overrun) and whether they are all zero.
    pub fn trailing(&self) -> (u64, bool) {
        let pos = self.pos();
        let len = self.bit_len();
        if pos >= len {
            return (0, true);
        }
        let mut r = self.clone();
        let mut left = len - pos;
        let n = left;
        let mut zero = true;
        while left > 0 {
            let k = left.min(32) as u32;
            zero &= r.read(k) == 0;
            left -= k as u64;
        }
        (n, zero)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Packs an MSB-first bit string ("0101…", spaces ignored) into AKC word order.
    pub fn pack(bits: &str) -> Vec<u8> {
        let bits: Vec<u8> = bits
            .bytes()
            .filter(|b| *b == b'0' || *b == b'1')
            .map(|b| b - b'0')
            .collect();
        let mut out = Vec::new();
        for chunk in bits.chunks(16) {
            let mut w: u16 = 0;
            for (i, b) in chunk.iter().enumerate() {
                w |= (*b as u16) << (15 - i);
            }
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }

    /// Builds bit strings field by field.
    #[derive(Default)]
    pub struct BitWriter {
        pub bits: String,
    }

    impl BitWriter {
        pub fn put(&mut self, n: u32, v: u64) -> &mut Self {
            for i in (0..n).rev() {
                self.bits.push(if (v >> i) & 1 == 1 { '1' } else { '0' });
            }
            self
        }
        pub fn num(&mut self, v: u64) -> &mut Self {
            if v == 0 {
                return self.put(5, 0);
            }
            let w = 64 - (v + 1).leading_zeros() - 1; // floor(log2(v+1))
            self.put(5, w as u64).put(w, v - ((1 << w) - 1))
        }
        pub fn len(&self) -> usize {
            self.bits.len()
        }
        pub fn bytes(&self) -> Vec<u8> {
            pack(&self.bits)
        }
    }

    #[test]
    fn word_order_and_straddling() {
        // Words 0x23c0 0x086e (bytes c0 23 6e 08), from spec §10.2.
        let data = [0xc0, 0x23, 0x6e, 0x08];
        let mut r = BitReader::new(&data).unwrap();
        assert_eq!(r.read(1), 0);
        assert_eq!(r.read(5), 8);
        assert_eq!(r.read(8), 240);
        assert_eq!(r.pos(), 14);
        assert_eq!(r.read(5), 0);
        assert_eq!(r.read(5), 8);
        assert_eq!(r.pos(), 24);
        assert!(!r.overrun());
        r.read(16);
        assert!(r.overrun());
    }

    #[test]
    fn num_and_seek() {
        let mut w = BitWriter::default();
        for v in [0u64, 1, 2, 3, 6, 7, 365, 2593, 4_000_000_000] {
            w.num(v);
        }
        let data = w.bytes();
        let mut r = BitReader::new(&data).unwrap();
        for v in [0u64, 1, 2, 3, 6, 7, 365, 2593, 4_000_000_000] {
            assert_eq!(r.num(), v);
        }
        let mut w = BitWriter::default();
        w.put(7, 0).put(32, 0xDEAD_BEEF).put(3, 5);
        let data = w.bytes();
        let mut r = BitReader::at(&data, 7).unwrap();
        assert_eq!(r.read(32), 0xDEAD_BEEF);
        assert_eq!(r.read(3), 5);
        assert_eq!(r.trailing(), (6, true));
        assert!(BitReader::new(&[1, 2, 3]).is_err());
    }
}
