//! Shared and block dictionaries (spec §4.1, §6).

use super::bits::BitReader;
use super::huffman::{Huffman, LengthsMode, read_lengths_mode};
use anyhow::{Result, bail, ensure};

/// Number of word classes.
pub const CLASSES: usize = 32;

/// Extra bits that select a word inside class `c` (§4.1).
#[inline]
pub const fn class_extra(c: u32) -> u32 {
    if c < 12 { 0 } else { c - 11 }
}

/// First word number of class `c`, for `c` in `0 ..= 32`.
pub const CLASS_FIRST: [u32; CLASSES + 1] = {
    let mut t = [0u32; CLASSES + 1];
    let mut c = 0;
    while c < CLASSES {
        t[c + 1] = t[c] + (1 << class_extra(c as u32));
        c += 1;
    }
    t
};

/// Token alphabet: 32 block classes, 32 shared classes, 256 match codes.
pub const TOKEN_SYMBOLS: usize = 320;
pub const PREFIX_SYMBOLS: usize = 64;
/// Character alphabet: 256 bytes plus 32 end-of-word-with-class symbols.
pub const CHAR_SYMBOLS: usize = 288;

/// How a word is rendered by the emitter (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Word = 0,
    Raw = 1,
    SentenceEnd = 2,
    StartTag = 3,
    AttrName = 4,
    BareValue = 5,
    SingleQuoted = 6,
    DoubleQuoted = 7,
    EndTagNamed = 8,
    EndTag = 9,
    EmptyClose = 10,
}

impl Kind {
    pub fn from_byte(b: u8) -> Option<Kind> {
        use Kind::*;
        Some(match b {
            0 => Word,
            1 => Raw,
            2 => SentenceEnd,
            3 => StartTag,
            4 => AttrName,
            5 => BareValue,
            6 => SingleQuoted,
            7 => DoubleQuoted,
            8 => EndTagNamed,
            9 => EndTag,
            10 => EmptyClose,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Word {
    pub start: u32,
    pub len: u32,
    pub kind: Kind,
}

/// A decoded word list: words by word number, their text in one buffer.
#[derive(Debug, Clone, Default)]
pub struct Words {
    pub words: Vec<Word>,
    pub text: Vec<u8>,
}

impl Words {
    pub fn len(&self) -> usize {
        self.words.len()
    }
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
    #[inline]
    pub fn get(&self, i: u32) -> (Kind, &[u8]) {
        let w = &self.words[i as usize];
        (
            w.kind,
            &self.text[w.start as usize..(w.start + w.len) as usize],
        )
    }
}

/// Header counts common to both dictionary types.
#[derive(Debug, Clone, Copy, Default)]
pub struct DictHeader {
    pub a: u64,
    pub x: u64,
    pub wc: u64,
    pub total: u64,
    pub maxlen: u64,
}

/// Counters for the `info` command.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodeStats {
    pub pretree: u32,
    pub sparse: u32,
    pub flat: u32,
    pub reused: u32,
}

impl CodeStats {
    fn note(&mut self, m: LengthsMode) {
        match m {
            LengthsMode::PreTree => self.pretree += 1,
            LengthsMode::Sparse => self.sparse += 1,
            LengthsMode::Flat => self.flat += 1,
        }
    }
    pub fn add(&mut self, o: &CodeStats) {
        self.pretree += o.pretree;
        self.sparse += o.sparse;
        self.flat += o.flat;
        self.reused += o.reused;
    }
}

/// The package-wide dictionary at header offset 0x1C.
#[derive(Debug, Clone)]
pub struct SharedDict {
    /// Capitalisation enabled (`C`).
    pub capitalize: bool,
    /// Self-closing tags allowed (`S`).
    pub self_closing: bool,
    pub header: DictHeader,
    pub token_code: Huffman,
    pub prefix_code: Huffman,
    pub even_code: Huffman,
    pub odd_code: Huffman,
    pub words: Words,
    pub stats: CodeStats,
    /// Bits left after the parse, and whether they are all zero.
    pub trailing_bits: (u64, bool),
}

/// The dictionary at the start of a compressed block.
#[derive(Debug, Clone, Default)]
pub struct BlockDict {
    pub header: DictHeader,
    /// `None` = use the shared token code.
    pub token_code: Option<Huffman>,
    pub words: Words,
    pub stats: CodeStats,
}

fn read_header(r: &mut BitReader) -> DictHeader {
    let a = r.num();
    read_header_after_a(r, a)
}

fn read_header_after_a(r: &mut BitReader, a: u64) -> DictHeader {
    DictHeader {
        a,
        x: r.num(),
        wc: r.num(),
        total: r.num(),
        maxlen: r.num(),
    }
}

fn read_code(r: &mut BitReader, n: usize, stats: &mut CodeStats) -> Result<Huffman> {
    let (lengths, mode) = read_lengths_mode(r, n)?;
    r.check("code lengths")?;
    stats.note(mode);
    Huffman::new(&lengths)
}

/// Number of classes holding `wc` words.
fn class_count(wc: u64) -> Result<usize> {
    ensure!(
        wc <= CLASS_FIRST[CLASSES] as u64,
        "word count {wc} exceeds class table"
    );
    Ok((0..CLASSES)
        .find(|&c| CLASS_FIRST[c + 1] as u64 >= wc)
        .unwrap()
        + 1)
}

fn read_class_sizes(r: &mut BitReader, h: &DictHeader) -> Result<Vec<u64>> {
    let n = class_count(h.wc)?;
    let sizes: Vec<u64> = (0..n).map(|_| r.num()).collect();
    let sum: u64 = sizes.iter().sum();
    ensure!(
        sum == h.total,
        "class sizes sum to {sum}, header total is {}",
        h.total
    );
    Ok(sizes)
}

struct WordCodes<'a> {
    prefix: &'a Huffman,
    even: &'a Huffman,
    odd: &'a Huffman,
}

/// Reads the front-coded word list (§6.3).
fn read_words(
    r: &mut BitReader,
    h: &DictHeader,
    sizes: &[u64],
    codes: WordCodes,
    self_closing: bool,
) -> Result<Words> {
    let wc = h.wc as usize;
    let mut words = vec![
        Word {
            start: 0,
            len: 0,
            kind: Kind::Word
        };
        wc
    ];
    let mut text = Vec::with_capacity(h.total.min(1 << 24) as usize);
    let mut fill = [0u32; CLASSES];
    let mut class_bytes = [0u64; CLASSES];
    // Current word buffer: byte 0 is the kind.
    let mut buf: Vec<u8> = Vec::with_capacity(h.maxlen.min(1 << 16) as usize + 1);
    for i in 0..wc {
        r.check("word list")?;
        let p = if i == 0 {
            0
        } else {
            let q = codes.prefix.decode(r)?;
            if q < 32 {
                q as u64
            } else {
                let k = q - 32;
                (1u64 << k) + 31 + r.read(k) as u64
            }
        };
        ensure!(
            p as usize <= buf.len(),
            "word {i}: prefix {p} longer than previous word ({})",
            buf.len()
        );
        buf.truncate(p as usize);
        let class = loop {
            let code = if buf.len().is_multiple_of(2) {
                codes.even
            } else {
                codes.odd
            };
            let t = code.decode(r)?;
            if t >= 256 {
                break t - 256;
            }
            buf.push(t as u8);
            if buf.len() as u64 > h.maxlen {
                r.check("word list")?;
                bail!("word {i} longer than maxlen {}", h.maxlen);
            }
        };
        ensure!(!buf.is_empty(), "word {i} is empty");
        let kind =
            Kind::from_byte(buf[0]).ok_or_else(|| anyhow::anyhow!("word {i}: kind {}", buf[0]))?;
        let l = buf.len();
        let ok = match kind {
            Kind::BareValue | Kind::SingleQuoted | Kind::DoubleQuoted => true,
            Kind::EndTag => l == 3, // a big-endian u16 ≤ 255 is a high byte of 0
            Kind::EmptyClose => l == 1 && self_closing,
            _ => l >= 2,
        };
        ensure!(
            ok && (kind != Kind::EndTag || buf[1] == 0),
            "word {i}: invalid {kind:?} of length {l}"
        );
        let c = class as usize;
        ensure!(c < sizes.len(), "word {i}: class {c} beyond last class");
        let cap = (CLASS_FIRST[c + 1] as u64).min(h.wc) - CLASS_FIRST[c] as u64;
        ensure!(
            (fill[c] as u64) < cap && (fill[c] as u64) < h.a,
            "word {i}: class {c} is full"
        );
        let tl = (l - 1) as u64;
        class_bytes[c] += tl;
        ensure!(
            class_bytes[c] <= sizes[c],
            "word {i}: class {c} text exceeds {}",
            sizes[c]
        );
        ensure!(
            (text.len() as u64 + tl) <= h.total,
            "word {i}: text exceeds total {}",
            h.total
        );
        let n = CLASS_FIRST[c] + fill[c];
        fill[c] += 1;
        words[n as usize] = Word {
            start: text.len() as u32,
            len: tl as u32,
            kind,
        };
        text.extend_from_slice(&buf[1..]);
    }
    ensure!(
        text.len() as u64 == h.total,
        "word text {} bytes, header total {}",
        text.len(),
        h.total
    );
    r.check("word list")?;
    Ok(Words { words, text })
}

impl SharedDict {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = BitReader::new(data)?;
        ensure!(r.bit(), "shared dictionary mode bit is 0");
        let capitalize = r.bit();
        let self_closing = r.bit();
        let header = read_header(&mut r);
        let mut stats = CodeStats::default();
        let token_code = read_code(&mut r, TOKEN_SYMBOLS, &mut stats)?;
        let sizes = if header.wc > 0 {
            read_class_sizes(&mut r, &header)?
        } else {
            Vec::new()
        };
        let prefix_code = read_code(&mut r, PREFIX_SYMBOLS, &mut stats)?;
        let even_code = read_code(&mut r, CHAR_SYMBOLS, &mut stats)?;
        let odd_code = read_code(&mut r, CHAR_SYMBOLS, &mut stats)?;
        let words = if header.wc > 0 {
            read_words(
                &mut r,
                &header,
                &sizes,
                WordCodes {
                    prefix: &prefix_code,
                    even: &even_code,
                    odd: &odd_code,
                },
                self_closing,
            )?
        } else {
            Words::default()
        };
        r.check("shared dictionary")?;
        Ok(SharedDict {
            capitalize,
            self_closing,
            header,
            token_code,
            prefix_code,
            even_code,
            odd_code,
            words,
            stats,
            trailing_bits: r.trailing(),
        })
    }
}

/// Reads a code or, if the reuse bit is set, borrows the shared one.
fn own_or_shared<'a>(
    r: &mut BitReader,
    n: usize,
    shared: &'a Huffman,
    stats: &mut CodeStats,
) -> Result<std::borrow::Cow<'a, Huffman>> {
    if r.bit() {
        stats.reused += 1;
        Ok(std::borrow::Cow::Borrowed(shared))
    } else {
        Ok(std::borrow::Cow::Owned(read_code(r, n, stats)?))
    }
}

impl BlockDict {
    /// Parses the dictionary at the start of a block, leaving `r` at the token stream.
    pub fn parse(r: &mut BitReader, shared: &SharedDict) -> Result<Self> {
        ensure!(!r.bit(), "block dictionary mode bit is 1");
        let a = r.num();
        if a == 0 {
            // No dictionary: shared token code, no block words (§6.1).
            return Ok(BlockDict::default());
        }
        let header = read_header_after_a(r, a);
        let mut stats = CodeStats::default();
        let token_code = if r.bit() {
            stats.reused += 1;
            None
        } else {
            Some(read_code(r, TOKEN_SYMBOLS, &mut stats)?)
        };
        if header.wc == 0 {
            return Ok(BlockDict {
                header,
                token_code,
                words: Words::default(),
                stats,
            });
        }
        let sizes = read_class_sizes(r, &header)?;
        let prefix = own_or_shared(r, PREFIX_SYMBOLS, &shared.prefix_code, &mut stats)?;
        let even = own_or_shared(r, CHAR_SYMBOLS, &shared.even_code, &mut stats)?;
        let odd = own_or_shared(r, CHAR_SYMBOLS, &shared.odd_code, &mut stats)?;
        let words = read_words(
            r,
            &header,
            &sizes,
            WordCodes {
                prefix: &prefix,
                even: &even,
                odd: &odd,
            },
            shared.self_closing,
        )?;
        Ok(BlockDict {
            header,
            token_code,
            words,
            stats,
        })
    }
}
