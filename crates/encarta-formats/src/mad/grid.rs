//! Grid sections: the serialised field model and the field codec (mad.md §4).

use super::bits::BitReader;
use super::huffman::{HuffTable, SymbolWidth};
use super::index::Index;
use anyhow::{Context, Result, bail, ensure};

/// First symbol value that is a phrase reference rather than a literal.
const PHRASE_BASE: u16 = 0xFEFB;
const LAST_PHRASE: u16 = 0xFFF9;
const RUN_BEFORE_LAST: u16 = 0xFFFA;
const RUN_LAST: u16 = 0xFFFB;
const COPY_MIRRORED: u16 = 0xFFFC;
const COPY_FORWARD: u16 = 0xFFFD;
const ESCAPE_16: u16 = 0xFFFE;
const ESCAPE_11: u16 = 0xFFFF;

/// The four tables a field uses in one context.
#[derive(Debug, Clone, Copy)]
struct ContextTables {
    distance: usize,
    length: usize,
    run: usize,
    symbol: usize,
}

#[derive(Debug, Clone)]
pub struct FieldModel {
    contexts: Vec<ContextTables>,
    pub filter: u8,
    pub sentinel: u16,
    pub modulus: u8,
    phrases: Vec<Vec<u16>>,
}

impl FieldModel {
    pub fn context_count(&self) -> usize {
        self.contexts.len()
    }
    pub fn phrase_count(&self) -> usize {
        self.phrases.len()
    }
}

/// The model at the start of a Grid section.
#[derive(Debug, Clone)]
pub struct GridModel {
    tables: Vec<HuffTable>,
    pub fields: Vec<FieldModel>,
    /// Serialised size in bytes (records start at or after it).
    pub size: usize,
}

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let Some(s) = self.b.get(self.pos..self.pos + n) else { bail!("grid model truncated at {}", self.pos) };
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

impl GridModel {
    pub fn parse(b: &[u8]) -> Result<GridModel> {
        let mut c = Cursor { b, pos: 0 };
        let ntables = c.u16()? as usize;
        let mut tables = Vec::with_capacity(ntables);
        for t in 0..ntables {
            let size = c.u32()? as usize;
            let blob = c.take(size)?;
            ensure!(blob.len() >= 4, "grid table {t} too short");
            let anchor = u32::from_le_bytes(blob[0..4].try_into().unwrap()) as usize;
            ensure!(anchor >= 4 && (anchor - 4).is_multiple_of(12), "grid table {t}: anchor {anchor} misaligned");
            let table = HuffTable::parse(blob, anchor, SymbolWidth::U16).with_context(|| format!("grid table {t}"))?;
            tables.push(table);
        }
        let nfields = c.u16()? as usize;
        let mut fields = Vec::with_capacity(nfields);
        for f in 0..nfields {
            let nctx = c.u8()? as usize;
            ensure!(nctx >= 1, "field {f} has no contexts");
            let filter = c.u8()?;
            let sentinel = c.u16()?;
            let modulus = c.u8()?;
            let ids = c.take(4 * nctx)?;
            let mut contexts = Vec::with_capacity(nctx);
            for k in 0..nctx {
                let t = &ids[4 * k..4 * k + 4];
                for &i in t {
                    ensure!((i as usize) < ntables, "field {f} context {k} names table {i} of {ntables}");
                }
                contexts.push(ContextTables {
                    distance: t[0] as usize,
                    length: t[1] as usize,
                    run: t[2] as usize,
                    symbol: t[3] as usize,
                });
            }
            let nphrases = c.u16()? as usize;
            let mut phrases = Vec::with_capacity(nphrases);
            for _ in 0..nphrases {
                let n = c.u16()? as usize;
                let values = c.take(2 * n)?;
                phrases.push(values.as_chunks::<2>().0.iter().map(|v| u16::from_le_bytes([v[0], v[1]])).collect());
            }
            fields.push(FieldModel { contexts, filter, sentinel, modulus, phrases });
        }
        Ok(GridModel { tables, fields, size: c.pos })
    }

    pub fn table_count(&self) -> usize {
        self.tables.len()
    }
}

/// A Grid section with its index: records are located by key.
pub struct Grid<'a> {
    pub name: String,
    pub model: GridModel,
    pub index: Index,
    data: &'a [u8],
}

impl<'a> Grid<'a> {
    pub fn new(name: &str, section: &'a [u8], index: Index) -> Result<Grid<'a>> {
        let model = GridModel::parse(section).with_context(|| format!("{name} model"))?;
        ensure!(
            index.data_end() <= section.len() as u64,
            "{name}: index points past the section end ({} > {})",
            index.data_end(),
            section.len()
        );
        Ok(Grid { name: name.to_string(), model, index, data: section })
    }

    /// Raw bytes of the record at `(offset, length)`.
    pub fn bytes(&self, offset: u32, length: u32) -> Result<&'a [u8]> {
        let (o, l) = (offset as usize, length as usize);
        ensure!(o >= self.model.size, "{}: record at {o} overlaps the model", self.name);
        self.data.get(o..o + l).with_context(|| format!("{}: record at {o}+{l} outside the section", self.name))
    }

    pub fn record_bytes(&self, key: u32) -> Option<&'a [u8]> {
        let (o, l) = self.index.lookup(key)?;
        self.bytes(o, l).ok()
    }

    /// A field decoder positioned at the start of the record for `key`.
    pub fn record(&self, key: u32) -> Result<Record<'_>> {
        let Some((o, l)) = self.index.lookup(key) else { bail!("{}: no record for key {key:#x}", self.name) };
        Ok(self.decoder(self.bytes(o, l)?))
    }

    pub fn decoder<'b>(&'b self, bytes: &'b [u8]) -> Record<'b> {
        Record { model: &self.model, bits: BitReader::new(bytes) }
    }
}

/// Decodes the fields of one record, in the order the table schema prescribes.
pub struct Record<'a> {
    model: &'a GridModel,
    bits: BitReader<'a>,
}

impl<'a> Record<'a> {
    pub fn new(model: &'a GridModel, bytes: &'a [u8]) -> Self {
        Record { model, bits: BitReader::new(bytes) }
    }

    fn field(&self, f: usize) -> Result<&'a FieldModel> {
        self.model.fields.get(f).with_context(|| format!("field {f} not in model ({} fields)", self.model.fields.len()))
    }

    fn get(&mut self, table: usize) -> Result<u32> {
        let s = self.model.tables[table].decode(&mut self.bits)?;
        Ok(match s {
            ESCAPE_11 => self.bits.read(11),
            ESCAPE_16 => self.bits.read(16),
            s => s as u32,
        })
    }

    /// Fills `out` with exactly `n` values of field `f`; byte fields keep only the low byte.
    /// Returns the context number modulo the field's modulus.
    pub fn decode_into(&mut self, f: usize, n: usize, byte: bool, out: &mut Vec<u16>) -> Result<u32> {
        out.clear();
        if n == 0 {
            return Ok(0);
        }
        let field = self.field(f)?;
        let nctx = field.contexts.len();
        let ctx = if nctx >= 2 {
            let bits = 32 - (nctx as u32 - 1).leading_zeros();
            let c = self.bits.read(bits) as usize;
            ensure!(c < nctx, "field {f}: context {c} of {nctx}");
            c
        } else {
            0
        };
        let t = field.contexts[ctx];
        let mask: u16 = if byte { 0xFF } else { 0xFFFF };
        out.reserve(n);
        while out.len() < n {
            let i = out.len();
            let s = self.get(t.symbol)?;
            if s < PHRASE_BASE as u32 {
                out.push(s as u16 & mask);
                continue;
            }
            let s = s as u16;
            match s {
                PHRASE_BASE..=LAST_PHRASE => {
                    let p = (s - PHRASE_BASE) as usize;
                    let phrase = field.phrases.get(p).with_context(|| format!("field {f}: phrase {p} missing"))?;
                    ensure!(i + phrase.len() <= n, "field {f}: phrase overruns {n} values");
                    out.extend(phrase.iter().map(|&v| v & mask));
                }
                RUN_BEFORE_LAST | RUN_LAST => {
                    let k = self.get(t.run)? as usize;
                    let back = if s == RUN_LAST { 1 } else { 2 };
                    ensure!(i >= back, "field {f}: run with no previous value");
                    ensure!(i + k <= n, "field {f}: run overruns {n} values");
                    let v = out[i - back];
                    out.resize(i + k, v);
                }
                COPY_MIRRORED | COPY_FORWARD => {
                    let d = self.get(t.distance)? as usize;
                    let k = self.get(t.length)? as usize;
                    ensure!(i + k <= n, "field {f}: copy overruns {n} values");
                    if s == COPY_MIRRORED {
                        // out[i-1-d-j] for j = 0…k-1
                        ensure!(k == 0 || i >= d + k, "field {f}: mirrored copy before the start");
                        for j in 0..k {
                            out.push(out[i - 1 - d - j]);
                        }
                    } else {
                        ensure!(k == 0 || i >= d + 2, "field {f}: copy before the start");
                        for j in 0..k {
                            out.push(out[i - 2 - d + j]);
                        }
                    }
                }
                _ => bail!("field {f}: invalid symbol {s:#x}"),
            }
        }
        match field.filter {
            0 => {}
            1 => {
                let sentinel = field.sentinel & mask;
                for i in 1..n {
                    if out[i] == sentinel {
                        out[i] = out[i - 1];
                    }
                }
            }
            2 if !byte => {
                for i in 1..n {
                    out[i] = (out[i] as u32 + out[i - 1] as u32 + 65536 - 32000) as u16;
                }
            }
            2 => {}
            other => bail!("field {f}: unknown post-filter {other}"),
        }
        Ok(if nctx < 2 { 0 } else { ctx as u32 % field.modulus.max(1) as u32 })
    }

    /// `W(f, n)`.
    pub fn words(&mut self, f: usize, n: usize) -> Result<Vec<u16>> {
        let mut v = Vec::new();
        self.decode_into(f, n, false, &mut v)?;
        Ok(v)
    }

    /// `W(f, n)` plus the field's return value.
    pub fn words_ctx(&mut self, f: usize, n: usize) -> Result<(Vec<u16>, u32)> {
        let mut v = Vec::new();
        let c = self.decode_into(f, n, false, &mut v)?;
        Ok((v, c))
    }

    /// `W1(f)`.
    pub fn word(&mut self, f: usize) -> Result<u16> {
        Ok(self.words(f, 1)?[0])
    }

    /// `B(f, n)`.
    pub fn bytes(&mut self, f: usize, n: usize) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        self.decode_into(f, n, true, &mut v)?;
        Ok(v.into_iter().map(|x| x as u8).collect())
    }

    /// `D(f, n)`: fields f…f+3 hold bytes 3 (MSB) … 0 of each u32.
    pub fn dwords(&mut self, f: usize, n: usize) -> Result<Vec<u32>> {
        let mut out = vec![0u32; n];
        let mut plane = Vec::new();
        for k in 0..4 {
            self.decode_into(f + k, n, true, &mut plane)?;
            let shift = 8 * (3 - k as u32);
            for (o, &b) in out.iter_mut().zip(&plane) {
                *o |= (b as u32) << shift;
            }
        }
        Ok(out)
    }

    /// Unread bits (negative if the decoder ran past the end).
    pub fn remaining_bits(&self) -> i64 {
        self.bits.remaining()
    }

    /// The integrity check every shipped record passes: 0…31 bits left over.
    pub fn finish(&self) -> Result<u32> {
        let r = self.bits.remaining();
        ensure!((0..32).contains(&r), "record ends with {r} unread bits (expected 0…31)");
        Ok(r as u32)
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Builds synthetic grid models. Each table is written as a flat code book: all symbols get
    //! `len`-bit codes (a single step), which keeps the encoder trivial.

    pub struct FieldSpec {
        pub nctx: u8,
        pub filter: u8,
        pub sentinel: u16,
        pub modulus: u8,
        /// Per context: distance, length, run, symbol table indices.
        pub tables: Vec<[u8; 4]>,
        pub phrases: Vec<Vec<u16>>,
    }

    impl FieldSpec {
        pub fn simple(table: u8) -> Self {
            FieldSpec { nctx: 1, filter: 0, sentinel: 0, modulus: 1, tables: vec![[table; 4]], phrases: vec![] }
        }
    }

    /// A table blob giving every symbol a `len`-bit code equal to its index.
    pub fn flat_table(len: u32, symbols: &[u16]) -> Vec<u8> {
        assert!(symbols.len() <= 1 << len);
        let mut b = 4u32.to_le_bytes().to_vec(); // the anchor is the single triplet
        b.extend(u32::MAX.to_le_bytes());
        b.extend(0i32.to_le_bytes());
        b.extend(len.to_le_bytes());
        b.extend((symbols.len() as u32).to_le_bytes());
        for s in symbols {
            b.extend(s.to_le_bytes());
        }
        b.extend([0, 0]);
        b
    }

    pub fn model(tables: &[Vec<u8>], fields: &[FieldSpec]) -> Vec<u8> {
        let mut b = (tables.len() as u16).to_le_bytes().to_vec();
        for t in tables {
            b.extend((t.len() as u32).to_le_bytes());
            b.extend(t);
        }
        b.extend((fields.len() as u16).to_le_bytes());
        for f in fields {
            b.push(f.nctx);
            b.push(f.filter);
            b.extend(f.sentinel.to_le_bytes());
            b.push(f.modulus);
            for t in &f.tables {
                b.extend(t);
            }
            b.extend((f.phrases.len() as u16).to_le_bytes());
            for p in &f.phrases {
                b.extend((p.len() as u16).to_le_bytes());
                for v in p {
                    b.extend(v.to_le_bytes());
                }
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use crate::mad::bits::testutil::BitWriter;

    /// Table 0: 1 bit → 11-bit or 16-bit escape (any value can be written).
    fn escapes() -> Vec<u8> {
        flat_table(1, &[ESCAPE_11, ESCAPE_16])
    }

    fn lit(w: &mut BitWriter, v: u32) {
        if v < 2048 {
            w.put(0, 1);
            w.put(v, 11);
        } else {
            w.put(1, 1);
            w.put(v, 16);
        }
    }

    #[test]
    fn literals_runs_copies_phrases() {
        let mut f0 = FieldSpec::simple(0);
        f0.phrases = vec![vec![9, 8], vec![0x1234]];
        let bytes_model = model(&[escapes()], &[f0, FieldSpec::simple(0)]);
        let m = GridModel::parse(&bytes_model).unwrap();
        assert_eq!(m.size, bytes_model.len());
        let mut w = BitWriter::default();
        // Field 0, 14 words: 5, 6, run-last ×2, run-before-last ×1, phrase 0, forward copy, mirrored copy.
        lit(&mut w, 5);
        lit(&mut w, 6);
        lit(&mut w, RUN_LAST as u32);
        lit(&mut w, 2); // → 5 6 6 6
        lit(&mut w, RUN_BEFORE_LAST as u32);
        lit(&mut w, 1); // → 5 6 6 6 6
        lit(&mut w, PHRASE_BASE as u32); // → … 9 8
        lit(&mut w, COPY_FORWARD as u32);
        lit(&mut w, 3); // d = 3: start at i-2-3 = 2
        lit(&mut w, 3); // k = 3 → out[2], out[3], out[4] = 6 6 6
        lit(&mut w, COPY_MIRRORED as u32);
        lit(&mut w, 0); // d = 0: out[i-1], out[i-2]
        lit(&mut w, 2);
        // i = 10: → out[9] = 6, out[8] = 6; then one phrase-1 value and a 16-bit literal.
        lit(&mut w, PHRASE_BASE as u32 + 1);
        lit(&mut w, 0xF000);
        // Field 1 as bytes: 0x1FF keeps 0xFF.
        lit(&mut w, 0x1FF);
        let used = w.bits();
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        let got = r.words(0, 14).unwrap();
        assert_eq!(got, [5, 6, 6, 6, 6, 9, 8, 6, 6, 6, 6, 6, 0x1234, 0xF000]);
        assert_eq!(r.bytes(1, 1).unwrap(), [0xFF]);
        assert_eq!(r.remaining_bits(), (data.len() * 8 - used) as i64);
        r.finish().unwrap();
    }

    #[test]
    fn contexts_and_modulus() {
        // Table 1: 2-bit codes for literals 10, 11, 12, 13.
        let t1 = flat_table(2, &[10, 11, 12, 13]);
        let f = FieldSpec { nctx: 3, filter: 0, sentinel: 0, modulus: 2, tables: vec![[0; 4], [1; 4], [1; 4]], phrases: vec![] };
        let m = GridModel::parse(&model(&[escapes(), t1], &[f])).unwrap();
        let mut w = BitWriter::default();
        w.put(2, 2); // context 2 (2 bits for 3 contexts)
        w.put(3, 2);
        w.put(0, 2);
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        let (v, ret) = r.words_ctx(0, 2).unwrap();
        assert_eq!(v, [13, 10]);
        assert_eq!(ret, 0); // 2 mod 2
        // n = 0 reads nothing, not even context bits.
        assert!(r.words(0, 0).unwrap().is_empty());
        assert_eq!(r.remaining_bits(), 26);
    }

    #[test]
    fn dword_planes_and_filters() {
        let mut fd = FieldSpec::simple(0);
        fd.filter = 2;
        let mut fs = FieldSpec::simple(0);
        fs.filter = 1;
        fs.sentinel = 0x77;
        let fields = vec![FieldSpec::simple(0), FieldSpec::simple(0), FieldSpec::simple(0), FieldSpec::simple(0), fd, fs];
        let m = GridModel::parse(&model(&[escapes()], &fields)).unwrap();
        let mut w = BitWriter::default();
        for plane in [[0x12, 0xAB], [0x34, 0xCD], [0x56, 0xEF], [0x78, 0x01]] {
            for v in plane {
                lit(&mut w, v);
            }
        }
        for v in [1000, 32001, 31999] {
            lit(&mut w, v);
        }
        for v in [3, 0x77, 0x77, 4] {
            lit(&mut w, v);
        }
        let data = w.finish();
        let mut r = Record::new(&m, &data);
        assert_eq!(r.dwords(0, 2).unwrap(), [0x1234_5678, 0xABCD_EF01]);
        assert_eq!(r.words(4, 3).unwrap(), [1000, 1001, 1000]);
        assert_eq!(r.bytes(5, 4).unwrap(), [3, 3, 3, 4]);
    }

    #[test]
    fn corrupt_streams_are_errors() {
        let m = GridModel::parse(&model(&[escapes()], &[FieldSpec::simple(0)])).unwrap();
        let mut w = BitWriter::default();
        lit(&mut w, 1);
        lit(&mut w, RUN_LAST as u32);
        lit(&mut w, 5); // overruns n = 3
        let data = w.finish();
        assert!(Record::new(&m, &data).words(0, 3).is_err());
        let mut w = BitWriter::default();
        lit(&mut w, RUN_LAST as u32); // nothing to repeat
        lit(&mut w, 1);
        let data = w.finish();
        assert!(Record::new(&m, &data).words(0, 1).is_err());
        let mut w = BitWriter::default();
        lit(&mut w, PHRASE_BASE as u32); // no phrases
        let data = w.finish();
        assert!(Record::new(&m, &data).words(0, 1).is_err());
    }
}
