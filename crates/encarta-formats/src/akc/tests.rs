//! End-to-end test on a small package built here from scratch.

use super::bits::tests::BitWriter;
use super::huffman::tests::canonical;
use super::*;

/// A complete code over `syms` (lengths k−1 / k), as per-symbol lengths of an `n`-symbol alphabet.
fn complete_lengths(n: usize, syms: &[usize]) -> Vec<u8> {
    let c = syms.len();
    let k = (c as f64).log2().ceil() as u8;
    let short = (1usize << k) - c;
    let mut l = vec![0u8; n];
    for (i, &s) in syms.iter().enumerate() {
        l[s] = if i < short { k - 1 } else { k };
    }
    l
}

/// Writes lengths in the flat mode (`0 0`, width 5).
fn put_flat(w: &mut BitWriter, lengths: &[u8]) {
    w.put(2, 0).put(3, 5);
    for &l in lengths {
        w.put(5, l as u64);
    }
}

struct Code(Vec<(u32, u32)>);
impl Code {
    fn new(lengths: &[u8]) -> Code {
        Code(canonical(lengths))
    }
    fn put(&self, w: &mut BitWriter, s: usize) {
        let (c, l) = self.0[s];
        assert!(l > 0, "symbol {s} not in code");
        w.put(l, c as u64);
    }
}

/// Front-codes `words` (kind byte + text) with one class per word (all words in classes < 12).
fn put_words(w: &mut BitWriter, words: &[(u8, &[u8])], prefix: &Code, even: &Code, odd: &Code) {
    let mut prev: Vec<u8> = Vec::new();
    for (i, (kind, text)) in words.iter().enumerate() {
        let mut buf = vec![*kind];
        buf.extend_from_slice(text);
        let p = if i == 0 {
            0
        } else {
            let p = prev
                .iter()
                .zip(&buf)
                .take_while(|(a, b)| a == b)
                .count()
                .min(1);
            prefix.put(w, p);
            p
        };
        for (j, &b) in buf.iter().enumerate().skip(p) {
            let code = if j % 2 == 0 { even } else { odd };
            code.put(w, b as usize);
        }
        let code = if buf.len() % 2 == 0 { even } else { odd };
        code.put(w, 256 + i);
        prev = buf;
    }
}

fn pad(w: &mut BitWriter) {
    while !w.len().is_multiple_of(16) {
        w.put(1, 0);
    }
}

const SHARED_WORDS: [(u8, &[u8]); 3] = [(3, b"p"), (9, &[0, 0]), (2, b". ")];
const BLOCK_WORDS: [(u8, &[u8]); 2] = [(0, b"hello"), (0, b"World")];

fn char_syms() -> Vec<usize> {
    let mut s: Vec<usize> = b"\x00\x02\x03\x09p. helloWorld"
        .iter()
        .map(|&b| b as usize)
        .collect();
    s.sort();
    s.dedup();
    s.extend(256..259);
    s
}

/// Shared dictionary: words 0 `<p`, 1 end tag, 2 `. `; flat-coded tables.
fn shared_dict() -> (Vec<u8>, Code, Code, Code) {
    let mut w = BitWriter::default();
    w.put(1, 1).put(1, 1).put(1, 1); // shared mode, C, S
    let total: usize = SHARED_WORDS.iter().map(|(_, t)| t.len()).sum();
    w.num(10).num(0).num(3).num(total as u64).num(3);
    let token = complete_lengths(320, &[0, 32, 33, 34]);
    put_flat(&mut w, &token);
    for (_, t) in SHARED_WORDS {
        w.num(t.len() as u64);
    }
    let prefix = complete_lengths(64, &[0, 1]);
    let chars = complete_lengths(288, &char_syms());
    put_flat(&mut w, &prefix);
    put_flat(&mut w, &chars);
    put_flat(&mut w, &chars);
    let (p, c) = (Code::new(&prefix), Code::new(&chars));
    put_words(&mut w, &SHARED_WORDS, &p, &c, &Code::new(&chars));
    pad(&mut w);
    (w.bytes(), Code::new(&token), p, c)
}

/// Block A: own dictionary (`hello`, `World`), two records of 6 tokens, the second a match.
fn block_a(shared_prefix: &Code, shared_chars: &Code) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.put(1, 0);
    let total: usize = BLOCK_WORDS.iter().map(|(_, t)| t.len()).sum();
    w.num(5).num(0).num(2).num(total as u64).num(6);
    // match: length code j = 4 (6 tokens), d = 2 → symbol 64 + 64 + 2 = 130
    let syms = [0, 1, 32, 33, 34, 130];
    let token = complete_lengths(320, &syms);
    w.put(1, 0);
    put_flat(&mut w, &token);
    for (_, t) in BLOCK_WORDS {
        w.num(t.len() as u64);
    }
    w.put(1, 1); // reuse shared prefix code
    w.put(1, 1); // reuse shared even code
    w.put(1, 0); // own odd code
    let odd = complete_lengths(288, &char_syms());
    put_flat(&mut w, &odd);
    put_words(
        &mut w,
        &BLOCK_WORDS,
        shared_prefix,
        shared_chars,
        &Code::new(&odd),
    );
    let tc = Code::new(&token);
    // <p hello World . hello </p>
    for s in [32, 0, 1, 34, 0, 33] {
        tc.put(&mut w, s);
    }
    tc.put(&mut w, 130);
    w.put(2, 2); // distance 4 + 2
    pad(&mut w);
    w.bytes()
}

/// Block B: no dictionary (A = 0), shared token code: `<p` `. ` end tag.
fn block_b(shared_token: &Code) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.put(1, 0).num(0);
    for s in [32, 34, 33] {
        shared_token.put(&mut w, s);
    }
    pad(&mut w);
    w.bytes()
}

/// Index: keys 0, 1 in block entry 0 (block A); key 2 direct (block B).
fn index(a: (u64, u64), b: (u64, u64), ulens: [u64; 3]) -> Vec<u8> {
    let fixed = 4 + 8 * 2 + 4 + 4; // N, 2 sparse pairs, M, 1 entry offset
    let mut w = BitWriter::default();
    w.put(5, 1);
    let desc = |w: &mut BitWriter, base: u64, width: u32, range: u64| {
        w.put(32, base).put(5, width as u64).put(width, range);
    };
    let wide = (1 << 24) - 1;
    desc(&mut w, 1, 0, 0);
    for _ in 0..4 {
        desc(&mut w, 0, 24, wide);
    }
    desc(&mut w, 0, 0, 0);
    desc(&mut w, 0, 1, 1);
    for _ in 0..4 {
        desc(&mut w, 0, 24, wide);
    }
    desc(&mut w, 0, 0, 0);
    let entry = fixed * 8 + w.len();
    // offset, clen, ulen_base, ulen_w, ntok_base, ntok_w, start_w
    w.put(24, a.0)
        .put(24, a.1)
        .put(24, 0)
        .put(5, 16)
        .put(24, 0)
        .put(5, 8)
        .put(5, 8);
    let g0 = fixed * 8 + w.len();
    w.put(1, 0).put(1, 0).put(16, ulens[0]).put(8, 6).put(8, 0);
    w.put(1, 0)
        .put(1, 0)
        .put(1, 0)
        .put(16, ulens[1])
        .put(8, 6)
        .put(8, 6);
    let g1 = fixed * 8 + w.len();
    w.put(1, 1)
        .put(24, b.0)
        .put(24, b.1)
        .put(24, ulens[2])
        .put(24, 3);
    pad(&mut w);
    let mut out = Vec::new();
    for v in [3u32, 0, g0 as u32, 2, g1 as u32, 1, entry as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&w.bytes());
    out
}

fn build_package() -> Vec<u8> {
    let label: Vec<u8> = "\u{FEFF}<label/>"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let (dict, token, prefix, chars) = shared_dict();
    let a = block_a(&prefix, &chars);
    let b = block_b(&token);
    let dict_off = 0x30 + label.len();
    let a_off = dict_off + dict.len();
    let b_off = a_off + a.len();
    let idx_off = b_off + b.len();
    let r0 = b"<p>Hello World. Hello</p>";
    let idx = index(
        (a_off as u64, a.len() as u64),
        (b_off as u64, b.len() as u64),
        [r0.len() as u64, r0.len() as u64, 9],
    );
    let mut f = Vec::new();
    f.extend_from_slice(&MAGIC.to_le_bytes());
    f.extend_from_slice(&1u32.to_le_bytes());
    f.extend_from_slice(&[
        0x88, 0xA2, 0xA8, 0x0E, 0x01, 0x02, 0x03, 0x04, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00,
        0x11,
    ]);
    for v in [
        0,
        dict_off as u32,
        dict.len() as u32,
        idx_off as u32,
        idx.len() as u32,
        label.len() as u32,
    ] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    f.extend_from_slice(&label);
    f.extend_from_slice(&dict);
    f.extend_from_slice(&a);
    f.extend_from_slice(&b);
    f.extend_from_slice(&idx);
    f
}

#[test]
fn synthetic_package() {
    let f = AkcFile::from_bytes(build_package()).unwrap();
    assert_eq!(f.header().label, "<label/>");
    assert_eq!(
        f.header().guid_string(),
        "{0EA8A288-0201-0403-AABB-CCDDEEFF0011}"
    );
    let s = f.shared().unwrap();
    assert!(s.capitalize && s.self_closing);
    assert_eq!(s.words.len(), 3);
    assert!(s.trailing_bits.1);
    assert!(s.trailing_bits.0 < 16);
    assert_eq!(f.len(), 3);
    assert_eq!(f.blocks().len(), 2);
    assert_eq!(f.blocks()[0].members, vec![0, 1]);
    assert!(f.blocks()[1].direct);

    let want0 = "<p>Hello World. Hello</p>";
    assert_eq!(f.record(0).unwrap(), want0.as_bytes());
    assert_eq!(f.record(1).unwrap(), want0.as_bytes());
    assert_eq!(f.record(2).unwrap(), b"<p>. </p>");
    assert!(f.get(3).unwrap().is_none());

    let b = f.decode_block(0).unwrap();
    assert_eq!(b.tokens.len(), 12);
    assert_eq!((b.stats.literals, b.stats.matches), (6, 1));
    assert!(b.trailing_bits.0 < 16 && b.trailing_bits.1);
    assert_eq!(b.dict.stats.reused, 2);

    let all: Vec<_> = f.iter().map(|r| r.unwrap()).collect();
    assert_eq!(all.len(), 3);
    let seen = Mutex::new(Vec::new());
    f.for_each_record(|pos, rec, bytes| {
        seen.lock().unwrap().push((pos, rec.key, bytes.len()));
        Ok(())
    })
    .unwrap();
    let mut seen = seen.into_inner().unwrap();
    seen.sort();
    assert_eq!(seen, vec![(0, 0, 25), (1, 1, 25), (2, 2, 9)]);

    let rep = check::check_file(&f, &check::Options::default());
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
    assert_eq!(rep.records, 3);
    assert_eq!(rep.bytes, 59);
    assert_eq!(rep.xml_ok, 3);
}

#[test]
fn corrupt_packages_fail_cleanly() {
    let good = build_package();
    // Flip every bit of the dictionary/blocks in turn: opening or decoding must never panic.
    let dict_off = u32::from_le_bytes(good[0x1C..0x20].try_into().unwrap()) as usize;
    let idx_off = u32::from_le_bytes(good[0x24..0x28].try_into().unwrap()) as usize;
    for byte in dict_off..idx_off + 40 {
        for bit in 0..8 {
            let mut bad = good.clone();
            bad[byte] ^= 1 << bit;
            if let Ok(f) = AkcFile::from_bytes(bad) {
                for k in 0..3 {
                    let _ = f.get(k);
                }
            }
        }
    }
    let mut stub = good[..0x30].to_vec();
    stub[0x18] = 1;
    stub[0x2C..0x30].fill(0);
    let f = AkcFile::from_bytes(stub).unwrap();
    assert!(f.header().is_stub() && f.is_empty());
    let mut bad = good.clone();
    bad[4] = 2;
    assert!(AkcFile::from_bytes(bad).is_err());
}

#[test]
fn xml_checker() {
    use check::check_xml;
    assert_eq!(
        check_xml(b"\xEF\xBB\xBF<?xml version=\"1.0\"?>\r\n<a refid=\"12\"><b/>x &amp; y</a>\r\n")
            .unwrap(),
        Some(12)
    );
    assert_eq!(
        check_xml(b"<list Classname=\"expert\"></list>").unwrap(),
        None
    );
    for bad in [
        &b"<a><b></a>"[..],
        b"<a></a><b></b>",
        b"<a>",
        b"text<a></a>",
        b"<a x='1' x='2'></a>",
        b"<a>&bogus;</a>",
        b"",
    ] {
        assert!(check_xml(bad).is_err(), "{}", String::from_utf8_lossy(bad));
    }
}
