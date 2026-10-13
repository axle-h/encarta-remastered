//! Megalith `*.LEX` lexicons (speller, dictionary and bilingual word lists). Spec: `docs/formats/lex.md`.
//!
//! A lexicon is a byte-coded DAWG. [`parse`] reads the header and tables; [`Lexicon::words`] walks the graph and
//! returns every word; [`Lexicon::scan`] checks the node stream's structure.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

pub const MAGIC: [u8; 4] = [0x20, 0xF3, 0x9F, 0x77];

/// Arc flag bits.
pub mod flag {
    pub const WORD_END: u16 = 0x001;
    pub const LAST: u16 = 0x002;
    pub const HAS_CHILD: u16 = 0x010;
    pub const CONTINUES: u16 = 0x020;
    pub const CHILD_NEXT: u16 = 0x040;
    pub const CHILD_SHARED: u16 = 0x080;
    pub const CHILD_ABSOLUTE: u16 = 0x100;
}

/// How indices into one table are written in the stream: values below `direct` are 1-byte codes, the rest of the
/// byte range are escape bytes that start a 2-byte code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CodeSpace {
    pub direct: u16,
    pub escapes: u8,
}

impl CodeSpace {
    fn capacity(self) -> usize {
        self.direct as usize + self.escapes as usize * 256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// One UTF-16 code unit.
    pub unit: u16,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Header {
    /// u16 at 0x04 (varies per file; meaning unknown).
    pub tag: u16,
    /// u16 at 0x06 (0x0100 on the disc).
    pub version: u16,
    /// u32 at 0x08 (64 on the disc).
    pub field_08: u32,
    pub lcids: Vec<u16>,
    pub copyright: String,
    /// The u16 that ends the copyright field (0x2D or 0; meaning unknown).
    pub trailing_unit: u16,
    /// Body words +0x00, +0x04, +0x0C (1, 0, 2 on the disc).
    pub body_words: [u32; 3],
    /// Body u16s at +0x10 and +0x12 (2 and 2 or 1 on the disc).
    pub body_halves: [u16; 2],
    pub max_word_len: u16,
    pub max_arcs: u16,
    pub word_count: u32,
    pub symbol_codes: CodeSpace,
    pub node_codes: CodeSpace,
    pub skip_codes: CodeSpace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lexicon {
    pub header: Header,
    #[serde(skip)]
    pub symbols: Vec<Symbol>,
    /// Shared-node table: stream offsets.
    #[serde(skip)]
    pub nodes: Vec<u32>,
    /// Skip table: byte distances.
    #[serde(skip)]
    pub skips: Vec<u32>,
    #[serde(skip)]
    pub stream: Vec<u8>,
}

/// Results of [`Lexicon::scan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScanStats {
    pub nodes: usize,
    pub arcs: usize,
    pub max_arcs: usize,
    pub symbols_used: usize,
    pub shared_nodes_used: usize,
    pub skips_used: usize,
}

fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    let s = b
        .get(o..o + 2)
        .with_context(|| format!("truncated at {o:#x}"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    let s = b
        .get(o..o + 4)
        .with_context(|| format!("truncated at {o:#x}"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn parse(b: &[u8]) -> Result<Lexicon> {
    ensure!(
        b.len() >= 0x20 && b[..4] == MAGIC,
        "not a Megalith lexicon (bad magic)"
    );
    let mut lcids = Vec::new();
    for i in 0..8 {
        let v = u16_at(b, 0x0C + 2 * i)?;
        if v == 0 || v == 0xCCCC {
            break;
        }
        lcids.push(v);
    }
    let units = u32_at(b, 0x1C)? as usize;
    ensure!(
        units >= 1 && 0x20 + 2 * units + 4 <= b.len(),
        "bad copyright length {units}"
    );
    let field: Vec<u16> = (0..units)
        .map(|i| u16_at(b, 0x20 + 2 * i))
        .collect::<Result<_>>()?;
    let text = &field[..units - 1];
    let text = &text[..text.iter().position(|&u| u == 0).unwrap_or(text.len())];
    let copyright = String::from_utf16(text).context("copyright is not valid UTF-16")?;
    let body = 0x20 + 2 * units + 4;
    let w = |o: usize| u32_at(b, body + o);
    let h = |o: usize| u16_at(b, body + o);
    let space = |o: usize| -> Result<CodeSpace> {
        let direct = u16_at(b, body + o)?;
        let escapes = *b.get(body + o + 3).context("truncated code descriptor")?;
        ensure!(
            direct as usize + escapes as usize == 256,
            "bad code descriptor at body+{o:#x}"
        );
        Ok(CodeSpace { direct, escapes })
    };
    let header = Header {
        tag: u16_at(b, 4)?,
        version: u16_at(b, 6)?,
        field_08: u32_at(b, 8)?,
        lcids,
        copyright,
        trailing_unit: field[units - 1],
        body_words: [w(0)?, w(4)?, w(0x0C)?],
        body_halves: [h(0x10)?, h(0x12)?],
        max_word_len: h(0x08)?,
        max_arcs: h(0x0A)?,
        word_count: w(0x14)?,
        symbol_codes: space(0x30)?,
        node_codes: space(0x34)?,
        skip_codes: space(0x38)?,
    };
    let (n_skip, n_sym, n_node) = (w(0x18)? as usize, w(0x1C)? as usize, w(0x24)? as usize);
    let (s_off, s_len) = (w(0x28)? as usize, w(0x2C)? as usize);
    ensure!(
        s_off == 0x3C + 4 * (n_sym + n_node + n_skip),
        "stream offset {s_off} does not follow the tables"
    );
    ensure!(
        body + s_off + s_len == b.len(),
        "stream does not end at end of file"
    );
    ensure!(
        header.symbol_codes.capacity() >= n_sym
            && header.node_codes.capacity() >= n_node
            && header.skip_codes.capacity() >= n_skip,
        "code space too small for its table"
    );
    let mut p = body + 0x3C;
    let symbols = (0..n_sym)
        .map(|i| {
            Ok(Symbol {
                unit: u16_at(b, p + 4 * i)?,
                flags: u16_at(b, p + 4 * i + 2)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    p += 4 * n_sym;
    let nodes = (0..n_node)
        .map(|i| u32_at(b, p + 4 * i))
        .collect::<Result<Vec<_>>>()?;
    p += 4 * n_node;
    let skips = (0..n_skip)
        .map(|i| u32_at(b, p + 4 * i))
        .collect::<Result<Vec<_>>>()?;
    p += 4 * n_skip;
    ensure!(
        nodes.iter().all(|&o| (o as usize) < s_len),
        "shared-node offset outside the stream"
    );
    Ok(Lexicon {
        header,
        symbols,
        nodes,
        skips,
        stream: b[p..].to_vec(),
    })
}

/// One decoded arc.
#[derive(Debug, Clone, Copy)]
struct Arc {
    unit: u16,
    flags: u16,
    /// Stream offset of the child node, if the arc has one.
    child: Option<usize>,
}

/// One decoded node: its arcs, the continuation nodes it names, and where it ends.
struct Node {
    arcs: Vec<Arc>,
    continuations: Vec<usize>,
    end: usize,
}

impl Lexicon {
    fn code(&self, space: CodeSpace, pos: &mut usize) -> Result<usize> {
        let byte = |p: usize| {
            self.stream
                .get(p)
                .copied()
                .with_context(|| format!("code runs past stream end at {p}"))
        };
        let first = byte(*pos)?;
        *pos += 1;
        if (first as u16) < space.direct {
            return Ok(first as usize);
        }
        let second = byte(*pos)?;
        *pos += 1;
        Ok(
            space.direct as usize
                + (first as usize - space.direct as usize) * 256
                + second as usize,
        )
    }

    fn shared(&self, index: usize) -> Result<usize> {
        self.nodes
            .get(index)
            .map(|&o| o as usize)
            .with_context(|| format!("shared-node index {index} out of range"))
    }

    fn node(&self, start: usize, mut mark: impl FnMut(usize, usize)) -> Result<Node> {
        let h = &self.header;
        let mut pos = start;
        let mut arcs = Vec::new();
        let mut continuations = Vec::new();
        let mut cursor: Option<usize> = None;
        let mut deferred_next = Vec::new();
        loop {
            let si = self.code(h.symbol_codes, &mut pos)?;
            let sym = *self
                .symbols
                .get(si)
                .with_context(|| format!("symbol {si} out of range at {pos}"))?;
            mark(0, si);
            let f = sym.flags;
            if f & flag::CONTINUES != 0 {
                let ni = self.code(h.node_codes, &mut pos)?;
                continuations.push(self.shared(ni)?);
                mark(1, ni);
            }
            let mut child = None;
            if f & flag::HAS_CHILD != 0 {
                if f & flag::CHILD_NEXT != 0 {
                    deferred_next.push(arcs.len());
                } else if f & flag::CHILD_SHARED != 0 {
                    let ni = self.code(h.node_codes, &mut pos)?;
                    child = Some(self.shared(ni)?);
                    mark(1, ni);
                } else if f & flag::CHILD_ABSOLUTE != 0 {
                    let s = self
                        .stream
                        .get(pos..pos + 3)
                        .context("absolute offset runs past stream end")?;
                    child = Some((s[0] as usize) << 16 | (s[1] as usize) << 8 | s[2] as usize);
                    pos += 3;
                } else {
                    let ki = self.code(h.skip_codes, &mut pos)?;
                    let skip = *self
                        .skips
                        .get(ki)
                        .with_context(|| format!("skip index {ki} out of range"))?;
                    mark(2, ki);
                    let c = cursor.unwrap_or(pos) + skip as usize;
                    cursor = Some(c);
                    child = Some(c);
                }
            }
            arcs.push(Arc {
                unit: sym.unit,
                flags: f,
                child,
            });
            if f & flag::LAST != 0 {
                break;
            }
        }
        for i in deferred_next {
            arcs[i].child = Some(pos);
        }
        for a in &arcs {
            if let Some(c) = a.child {
                ensure!(
                    c < self.stream.len(),
                    "child offset {c} outside the stream (node at {start})"
                );
            }
        }
        Ok(Node {
            arcs,
            continuations,
            end: pos,
        })
    }

    /// Every word, in traversal order (depth-first, arcs in stored order, continuation arcs after a node's own).
    pub fn words(&self) -> Result<Vec<String>> {
        let max_len = self.header.max_word_len as usize;
        let limit = self.header.word_count as usize;
        let mut out = Vec::with_capacity(limit);
        let mut prefix: Vec<u16> = Vec::new();
        self.walk(0, &mut prefix, max_len, limit, &mut out)?;
        ensure!(
            out.len() == limit,
            "decoded {} words, header says {limit}",
            out.len()
        );
        Ok(out)
    }

    fn walk(
        &self,
        start: usize,
        prefix: &mut Vec<u16>,
        max_len: usize,
        limit: usize,
        out: &mut Vec<String>,
    ) -> Result<()> {
        ensure!(
            prefix.len() < max_len,
            "word longer than the header maximum ({max_len})"
        );
        // A node's arcs, then the arcs of each continuation node, then theirs.
        let mut pending = vec![start];
        let mut hops = 0usize;
        while let Some(at) = pending.pop() {
            hops += 1;
            ensure!(hops <= self.nodes.len() + 1, "continuation loop at {at}");
            let node = self.node(at, |_, _| {})?;
            for a in &node.arcs {
                prefix.push(a.unit);
                if a.flags & flag::WORD_END != 0 {
                    ensure!(out.len() < limit, "more words than the header's {limit}");
                    out.push(String::from_utf16(prefix).context("word is not valid UTF-16")?);
                }
                if let Some(c) = a.child {
                    self.walk(c, prefix, max_len, limit, out)?;
                }
                prefix.pop();
            }
            pending.extend(node.continuations.iter().rev());
        }
        Ok(())
    }

    /// Linear scan of the node stream: nodes must tile it exactly, and every child and continuation offset must be a
    /// node start.
    pub fn scan(&self) -> Result<ScanStats> {
        let mut used: [HashSet<usize>; 3] = Default::default();
        let mut starts = HashSet::new();
        let mut targets = Vec::new();
        let (mut pos, mut arcs, mut max_arcs) = (0usize, 0usize, 0usize);
        while pos < self.stream.len() {
            starts.insert(pos);
            let node = self.node(pos, |table, i| {
                used[table].insert(i);
            })?;
            arcs += node.arcs.len();
            max_arcs = max_arcs.max(node.arcs.len());
            targets.extend(node.arcs.iter().filter_map(|a| a.child));
            targets.extend(node.continuations);
            pos = node.end;
        }
        ensure!(
            pos == self.stream.len(),
            "last node runs past the stream end"
        );
        if let Some(t) = targets.iter().find(|t| !starts.contains(t)) {
            bail!("offset {t} is not a node start");
        }
        Ok(ScanStats {
            nodes: starts.len(),
            arcs,
            max_arcs,
            symbols_used: used[0].len(),
            shared_nodes_used: used[1].len(),
            skips_used: used[2].len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a lexicon file around the given tables and stream (synthetic, for tests).
    fn build(
        symbols: &[(u16, u16)],
        nodes: &[u32],
        skips: &[u32],
        stream: &[u8],
        words: u32,
        max_len: u16,
    ) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.extend(0x0200u16.to_le_bytes());
        b.extend(0x0100u16.to_le_bytes());
        b.extend(64u32.to_le_bytes());
        b.extend(0x0409u16.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b.resize(0x1C, 0xCC);
        let text: Vec<u16> = "test".encode_utf16().collect();
        b.extend((text.len() as u32 + 2).to_le_bytes());
        for u in text.iter().chain(&[0u16, 0x2D]) {
            b.extend(u.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        let tables = 4 * (symbols.len() + nodes.len() + skips.len());
        for v in [1u32, 0] {
            b.extend(v.to_le_bytes());
        }
        b.extend(max_len.to_le_bytes());
        b.extend(8u16.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        for v in [
            words,
            skips.len() as u32,
            symbols.len() as u32,
            0,
            nodes.len() as u32,
        ] {
            b.extend(v.to_le_bytes());
        }
        b.extend((0x3C + tables as u32).to_le_bytes());
        b.extend((stream.len() as u32).to_le_bytes());
        // symbols: 254 direct + 2 escapes; nodes and skips: 256 direct.
        b.extend([254, 0, 0, 2]);
        b.extend([0, 1, 0, 0]);
        b.extend([0, 1, 0, 0]);
        for &(u, f) in symbols {
            b.extend(u.to_le_bytes());
            b.extend(f.to_le_bytes());
        }
        for v in nodes.iter().chain(skips) {
            b.extend(v.to_le_bytes());
        }
        b.extend(stream);
        b
    }

    #[test]
    fn decodes_every_child_kind() {
        use flag::*;
        let sym = [
            (b'c' as u16, HAS_CHILD),                     // 0: c, skip-coded child
            (b'd' as u16, HAS_CHILD | CHILD_SHARED),      // 1: d, shared child
            (b'b' as u16, HAS_CHILD | CHILD_ABSOLUTE),    // 2: b, absolute child
            (b'a' as u16, HAS_CHILD | CHILD_NEXT | LAST), // 3: a, child right after the node
            (b't' as u16, WORD_END | LAST),               // 4: t, word end, leaf
            (b'o' as u16, WORD_END | HAS_CHILD | CHILD_NEXT | LAST), // 5: o, word end + child
            (b'g' as u16, WORD_END | LAST | CONTINUES),   // 6: g, word end, siblings continue
        ];
        // root @0: c (skip 9 from offset 2 → 11), d (shared node 0 → 10), b (absolute → 14), a (child next → 9)
        // @9 t; @10 t; @11 o (child next → 12); @12 g, siblings continue at shared node 0 (@10)
        let stream = [
            0, 0, // c, skip index 0
            1, 0, // d, shared index 0
            2, 0, 0, 14, // b, absolute 14
            3,  // a
            4,  // @9  t
            4,  // @10 t
            5,  // @11 o
            6, 0, // @12 g, continues at shared node 0
            4, // @14 t
        ];
        let file = build(&sym, &[10], &[9], &stream, 6, 4);
        let lex = parse(&file).unwrap();
        assert_eq!(lex.header.copyright, "test");
        assert_eq!(lex.header.trailing_unit, 0x2D);
        assert_eq!(lex.header.lcids, vec![0x409]);
        let mut w = lex.words().unwrap();
        w.sort();
        assert_eq!(w, ["at", "bt", "co", "cog", "cot", "dt"]);
        let s = lex.scan().unwrap();
        assert_eq!(
            (
                s.nodes,
                s.max_arcs,
                s.symbols_used,
                s.shared_nodes_used,
                s.skips_used
            ),
            (6, 4, 7, 1, 1)
        );
    }

    #[test]
    fn two_byte_codes() {
        // 300 symbols: index 299 is the 2-byte code [254, 45] → 254 + 0*256 + 45.
        let mut sym = vec![(b'x' as u16, flag::WORD_END | flag::LAST); 300];
        sym[299] = (b'q' as u16, flag::WORD_END | flag::LAST);
        let file = build(&sym, &[], &[], &[254, 45], 1, 2);
        assert_eq!(parse(&file).unwrap().words().unwrap(), ["q"]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(b"not a lexicon at all, definitely not").is_err());
        let sym = [(b'a' as u16, flag::WORD_END | flag::LAST)];
        // header says 2 words, stream holds 1
        let lex = parse(&build(&sym, &[], &[], &[0], 2, 2)).unwrap();
        assert!(lex.words().is_err());
        // shared child pointing at a stream offset that is not a node start
        let sym = [
            (
                b'a' as u16,
                flag::HAS_CHILD | flag::CHILD_SHARED | flag::LAST,
            ),
            (b'b' as u16, 0x3),
        ];
        let lex = parse(&build(&sym, &[1], &[], &[0, 0, 1], 1, 3)).unwrap();
        assert!(lex.scan().is_err());
    }
}
