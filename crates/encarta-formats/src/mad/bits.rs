//! Bit reader shared by the grid field codec and the raster tile codec (mad.md §1).
//!
//! The stream is a sequence of little-endian u32 words, each consumed from its most significant
//! bit down. A trailing partial word (grid records only) has its bytes in the high-order end.

#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    next: usize,
    /// Pending bits, left-justified.
    buf: u64,
    have: u32,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut r = BitReader { data, next: 0, buf: 0, have: 0 };
        r.refill();
        r
    }

    fn word(&self, i: usize) -> u32 {
        let o = i * 4;
        if o + 4 <= self.data.len() {
            u32::from_le_bytes(self.data[o..o + 4].try_into().unwrap())
        } else if o < self.data.len() {
            // r trailing bytes b0..b(r-1) form the LE word of [0; 4-r] ++ [b0..b(r-1)].
            let tail = &self.data[o..];
            let shift = 8 * (4 - tail.len() as u32);
            tail.iter().enumerate().fold(0u32, |w, (k, &b)| w | (b as u32) << (shift + 8 * k as u32))
        } else {
            0
        }
    }

    #[inline]
    fn refill(&mut self) {
        while self.have <= 32 {
            self.buf |= (self.word(self.next) as u64) << (32 - self.have);
            self.have += 32;
            self.next += 1;
        }
    }

    /// The next 32 bits without consuming them; bits past the end read as 0.
    #[inline]
    pub fn peek32(&self) -> u32 {
        (self.buf >> 32) as u32
    }

    /// Consumes `n` (≤ 32) bits that were already looked at with `peek32`.
    #[inline]
    pub fn skip(&mut self, n: u32) {
        debug_assert!(n <= 32);
        self.buf <<= n;
        self.have -= n;
        if self.have <= 32 {
            self.refill();
        }
    }

    /// The next `n` (0…32) bits as an unsigned integer, first bit most significant.
    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = (self.buf >> (64 - n)) as u32;
        self.skip(n);
        v
    }

    /// Bits consumed so far.
    pub fn position(&self) -> u64 {
        self.next as u64 * 32 - self.have as u64
    }

    /// Data bits in the stream (8 per byte). The zero bits that pad a partial final word come
    /// after the data bits in reading order, so they are not counted.
    pub fn total_bits(&self) -> u64 {
        self.data.len() as u64 * 8
    }

    /// Unread data bits, negative when the decoder has read past the end of the data.
    pub fn remaining(&self) -> i64 {
        self.total_bits() as i64 - self.position() as i64
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Writes bits MSB-first into LE u32 words: the inverse of `BitReader` for whole words.
    #[derive(Default)]
    pub struct BitWriter {
        words: Vec<u32>,
        cur: u64,
        n: u32,
    }

    impl BitWriter {
        pub fn put(&mut self, value: u32, bits: u32) {
            for i in (0..bits).rev() {
                self.cur = (self.cur << 1) | ((value >> i) & 1) as u64;
                self.n += 1;
                if self.n == 32 {
                    self.words.push(self.cur as u32);
                    self.cur = 0;
                    self.n = 0;
                }
            }
        }

        pub fn bits(&self) -> usize {
            self.words.len() * 32 + self.n as usize
        }

        pub fn finish(mut self) -> Vec<u8> {
            if self.n > 0 {
                let pad = 32 - self.n;
                self.put(0, pad);
            }
            self.words.iter().flat_map(|w| w.to_le_bytes()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::BitWriter;
    use super::*;

    #[test]
    fn msb_first_le_words() {
        let data = 0x8000_0001u32.to_le_bytes();
        let mut r = BitReader::new(&data);
        assert_eq!(r.read(1), 1);
        assert_eq!(r.read(30), 0);
        assert_eq!(r.read(1), 1);
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.read(5), 0);
        assert!(r.remaining() < 0);
    }

    #[test]
    fn straddles_words_and_peeks() {
        let mut w = BitWriter::default();
        w.put(0x5, 3);
        w.put(0xDEADBEEF, 32);
        w.put(0x1234, 16);
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read(3), 5);
        assert_eq!(r.peek32(), 0xDEADBEEF);
        assert_eq!(r.read(32), 0xDEADBEEF);
        assert_eq!(r.read(16), 0x1234);
        assert_eq!(r.position(), 51);
        assert_eq!(r.read(0), 0);
    }

    #[test]
    fn partial_final_word_is_high_aligned() {
        // Five bytes: one full word, then one byte that lands in bits 31..24 of word 2.
        let data = [0, 0, 0, 0, 0xA5];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read(32), 0);
        assert_eq!(r.read(8), 0xA5);
        assert_eq!(r.total_bits(), 40);
        assert_eq!(r.remaining(), 0);
        // Three bytes x y z → LE word of [0, x, y, z] → z is read first.
        let mut r = BitReader::new(&[0x11, 0x22, 0x33]);
        assert_eq!(r.read(32), 0x3322_1100);
    }
}
