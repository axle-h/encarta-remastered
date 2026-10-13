//! Byte-level helpers shared by the Director readers: a bounds-checked cursor, four-character
//! codes, base-128 varints and zlib.

use anyhow::{Context, Result, bail, ensure};
use serde::{Serialize, Serializer};
use std::fmt;
use std::io::Read;

/// A four-character code in reading order (`CASt`, `KEY*`, `snd ` …).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FourCC(pub [u8; 4]);

impl FourCC {
    pub fn new(s: &str) -> Self {
        let mut b = [b' '; 4];
        for (d, s) in b.iter_mut().zip(s.bytes()) {
            *d = s;
        }
        FourCC(b)
    }

    /// From the byte-reversed form the little-endian container stores.
    pub fn reversed(b: &[u8]) -> Self {
        FourCC([b[3], b[2], b[1], b[0]])
    }

    pub fn as_string(&self) -> String {
        self.0
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '?'
                }
            })
            .collect()
    }
}

impl fmt::Display for FourCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl fmt::Debug for FourCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}'", self.as_string())
    }
}

impl Serialize for FourCC {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_string())
    }
}

pub fn inflate(b: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(b)
        .read_to_end(&mut out)
        .context("zlib")?;
    Ok(out)
}

/// Bounds-checked reader over a byte slice.
pub struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Cursor { b, p: 0 }
    }
    pub fn at(b: &'a [u8], p: usize) -> Self {
        Cursor { b, p }
    }
    pub fn pos(&self) -> usize {
        self.p
    }
    pub fn seek(&mut self, p: usize) {
        self.p = p;
    }
    pub fn remaining(&self) -> usize {
        self.b.len().saturating_sub(self.p)
    }
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
    pub fn rest(&self) -> &'a [u8] {
        &self.b[self.p.min(self.b.len())..]
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(
            n <= self.remaining(),
            "read of {n} bytes at {} past end ({})",
            self.p,
            self.b.len()
        );
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.arr::<1>()?[0])
    }
    pub fn u16_le(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    pub fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    pub fn i32_le(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.arr()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.arr()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.arr()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.arr()?))
    }
    /// Unsigned base-128, most significant group first; 0x80 = more bytes follow.
    pub fn varint(&mut self) -> Result<u32> {
        let mut v: u32 = 0;
        for _ in 0..5 {
            let b = self.u8()?;
            v = (v << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        bail!("varint longer than 5 bytes at {}", self.p)
    }
    /// NUL-terminated string (Latin-1 is enough for the compressor names).
    pub fn cstr(&mut self) -> Result<String> {
        let rest = self.rest();
        let n = rest
            .iter()
            .position(|&b| b == 0)
            .context("unterminated string")?;
        let s = rest[..n].iter().map(|&b| b as char).collect();
        self.p += n + 1;
        Ok(s)
    }
    pub fn expect_tag(&mut self, reading_order: &str) -> Result<()> {
        let got = FourCC::reversed(self.take(4)?);
        ensure!(
            got == FourCC::new(reading_order),
            "expected {reading_order} at {}, found {got}",
            self.p - 4
        );
        Ok(())
    }
}

/// Big-endian helpers over a slice at a fixed offset (`None` when out of range).
pub fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
pub fn be_i16(b: &[u8], at: usize) -> Option<i16> {
    be_u16(b, at).map(|v| v as i16)
}
pub fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
pub fn be_i32(b: &[u8], at: usize) -> Option<i32> {
    be_u32(b, at).map(|v| v as i32)
}

/// Mac Roman → Unicode (member names were authored on a Mac).
pub fn mac_roman(b: &[u8]) -> String {
    const HIGH: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»…\u{A0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{F8FF}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
    let high: Vec<char> = HIGH.chars().collect();
    b.iter()
        .map(|&c| match c {
            0x0d => '\n',
            c if c < 0x80 => c as char,
            c => high[(c - 0x80) as usize],
        })
        .collect()
}

/// Windows-1252 → Unicode, carriage returns as `\n` (field text).
pub fn windows_1252(b: &[u8]) -> String {
    let (s, _, _) = encoding_rs::WINDOWS_1252.decode(b);
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_groups() {
        let mut c = Cursor::new(&[0x8a, 0x01, 0x8e, 0x3a, 0x00, 0x8f, 0xff, 0xff, 0xff, 0x7f]);
        assert_eq!(c.varint().unwrap(), 0x501);
        assert_eq!(c.varint().unwrap(), 1850);
        assert_eq!(c.varint().unwrap(), 0);
        assert_eq!(c.varint().unwrap() as i32, -1);
    }

    #[test]
    fn fourcc_reversal() {
        assert_eq!(FourCC::reversed(b"tSAC").to_string(), "CASt");
        assert_eq!(FourCC::new("snd").to_string(), "snd ");
    }

    #[test]
    fn mac_roman_table_is_complete() {
        assert_eq!(mac_roman(&[0x42, 0x6f, 0x67, 0x6f, 0x74, 0x87]), "Bogotá");
        assert_eq!(mac_roman(&[0xff]), "ˇ");
        assert_eq!(mac_roman(&[0xca]), "\u{A0}");
        let all: Vec<u8> = (0x80..=0xff).collect();
        assert_eq!(mac_roman(&all).chars().count(), 128);
    }
}
