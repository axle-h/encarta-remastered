//! The G1G2 shifting 8-bit place-name encoding (mad.md §8.3).

use anyhow::{Result, ensure};

/// Character maps loaded from the `G1G2 to Unicode` table.
#[derive(Clone)]
pub struct G1G2 {
    maps: [[char; 256]; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shift {
    Map(usize),
    Raw,
    Japanese,
}

impl G1G2 {
    /// Parses `n × {u8 set, u8 byte, u16 unicode}`. Map 0 starts as the identity, maps 1 and 2 as
    /// spaces; sets other than 1 and 2 write map 0.
    pub fn parse(b: &[u8]) -> Result<G1G2> {
        ensure!(b.len().is_multiple_of(4), "G1G2 table length {} is not a multiple of 4", b.len());
        let mut maps = [[' '; 256]; 3];
        for (i, c) in maps[0].iter_mut().enumerate() {
            *c = char::from(i as u8);
        }
        for r in b.as_chunks::<4>().0.iter() {
            let set = match r[0] {
                1 => 1,
                2 => 2,
                _ => 0,
            };
            let u = u16::from_le_bytes([r[2], r[3]]) as u32;
            maps[set][r[1] as usize] = char::from_u32(u).unwrap_or('\u{FFFD}');
        }
        Ok(G1G2 { maps })
    }

    /// Decodes up to the first NUL (or the end of `bytes`).
    pub fn decode(&self, bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len());
        let mut shift = Shift::Map(0);
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            i += 1;
            match c {
                0 => break,
                1 => shift = Shift::Raw,
                2..=4 => shift = Shift::Map((c - 2) as usize),
                5 => shift = Shift::Japanese,
                8 | 9 => {
                    shift = Shift::Map(0);
                    out.push(self.maps[0][c as usize]);
                }
                _ => match shift {
                    Shift::Map(m) => out.push(self.maps[m][c as usize]),
                    Shift::Raw => out.push(char::from(c)),
                    Shift::Japanese => {
                        let lead = matches!(c, 0x81..=0x9F | 0xE0..=0xFC);
                        if lead && i < bytes.len() && bytes[i] != 0 {
                            let (s, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[i - 1..i + 1]);
                            out.push_str(&s);
                            i += 1;
                        } else {
                            let (s, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[i - 1..i]);
                            out.push_str(&s);
                        }
                    }
                },
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> G1G2 {
        let mut b = Vec::new();
        for (set, byte, u) in [(0u8, 0x92u8, 0x2019u16), (1, 0x41, 0x0101), (2, 0x41, 0x0394), (7, 0x93, 0x201C)] {
            b.extend([set, byte]);
            b.extend(u.to_le_bytes());
        }
        G1G2::parse(&b).unwrap()
    }

    #[test]
    fn shifts_between_maps() {
        let t = table();
        assert_eq!(t.decode(b"Ab\x03A\x02c"), "Abāc");
        assert_eq!(t.decode(b"\x92x\x04A\x03B"), "’xΔ ");
        assert_eq!(t.decode(b"\x93q\0ignored"), "“q");
        // 0x08 resets to map 0 and is itself output.
        assert_eq!(t.decode(b"\x03A\x08A"), "ā\u{8}A");
        // Raw mode: bytes are code points.
        assert_eq!(t.decode(b"\x01\xE9"), "é");
    }

    #[test]
    fn shift_jis_mode() {
        let t = table();
        // "日" in Shift-JIS is 93 FA.
        assert_eq!(t.decode(b"\x05\x93\xFAa"), "日a");
    }
}
