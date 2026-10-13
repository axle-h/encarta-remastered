//! Readers for the "Find" search exports in `FINDDLX.CAB` (Deluxe `…DLX…`, Kids `…KDC…`).
//!
//! Spec: `docs/formats/search-catalog-misc.md` §1. Files:
//!
//! * `PFX*.DAT`   word-wheel title lists ([`Pfx`])
//! * `IdXE*.EFF`  search-result records, 16-byte aligned ([`IdxE`])
//! * `IdxW*.EFF`  vocabulary B-tree ([`IdxW`])
//! * `IdXEW*.EFF` postings per word ([`IdxEw`])
//! * `IdXCM*.ECT` normalisation character map ([`CharMap`])
//!
//! All integers are little-endian. A "net string" is a 7-bit varint byte length then the bytes.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Bounds-checked little-endian cursor over a byte slice.
#[derive(Clone)]
pub struct Cursor<'a> {
    pub b: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(b: &'a [u8], pos: usize) -> Self {
        Cursor { b, pos }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.with_context(|| {
            format!(
                "read of {n} bytes at {:#x} past end ({:#x})",
                self.pos,
                self.b.len()
            )
        })?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u24(&mut self) -> Result<u32> {
        let s = self.take(3)?;
        Ok(s[0] as u32 | (s[1] as u32) << 8 | (s[2] as u32) << 16)
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    /// 7-bit varint, low group first, high bit = continuation (at most 5 bytes).
    pub fn varint(&mut self) -> Result<u32> {
        let mut v: u32 = 0;
        for shift in (0..35).step_by(7) {
            let c = self.u8()?;
            v |= ((c & 0x7F) as u32) << shift;
            if c & 0x80 == 0 {
                return Ok(v);
            }
        }
        bail!("varint longer than 5 bytes at {:#x}", self.pos)
    }

    /// Net string with UTF-8 bytes.
    pub fn str_utf8(&mut self) -> Result<String> {
        let n = self.varint()? as usize;
        let at = self.pos;
        let s = self.take(n)?;
        String::from_utf8(s.to_vec()).with_context(|| format!("invalid UTF-8 string at {at:#x}"))
    }

    /// Net string whose byte length covers UTF-16LE code units.
    pub fn str_utf16(&mut self) -> Result<String> {
        let n = self.varint()? as usize;
        ensure!(n.is_multiple_of(2), "odd UTF-16 string length {n}");
        let units: Vec<u16> = self
            .take(n)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16(&units).context("invalid UTF-16 string")
    }
}

/// Header shared by every `.EFF`: a copyright net string, then `major, minor, rev` bytes.
#[derive(Debug, Clone, Serialize)]
pub struct EffHeader {
    pub copyright: String,
    pub version: [u8; 3],
    /// Offset just past the version bytes.
    pub end: usize,
}

impl EffHeader {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(b, 0);
        let copyright = c.str_utf8()?;
        ensure!(
            copyright.starts_with("Copyright"),
            "not an EFF file (header {copyright:?})"
        );
        let v = c.take(3)?;
        Ok(EffHeader {
            copyright,
            version: [v[0], v[1], v[2]],
            end: c.pos,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// PFX

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PfxEntry {
    pub ref_id: u32,
    pub class_id: u16,
    pub sort_title: String,
    pub display_title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PfxCollection {
    pub lcid: u32,
    pub entries: Vec<PfxEntry>,
}

/// Word-wheel file: collections back to back, each `i32 count, i32 lcid, count × entry`.
pub struct Pfx {
    pub collections: Vec<PfxCollection>,
}

impl Pfx {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(b, 0);
        let mut collections = Vec::new();
        while c.pos < b.len() {
            let n = c.i32()?;
            ensure!(n >= 0, "negative PFX count {n} at {:#x}", c.pos - 4);
            let lcid = c.u32()?;
            let mut entries = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let ref_id = c.u32()?;
                let class_id = c.u16()?;
                let sort_title = c.str_utf8()?;
                let display_title = c.str_utf8()?;
                entries.push(PfxEntry {
                    ref_id,
                    class_id,
                    sort_title,
                    display_title,
                });
            }
            collections.push(PfxCollection { lcid, entries });
        }
        Ok(Pfx { collections })
    }
}

// ---------------------------------------------------------------------------------------------
// IdXE

/// One search-result record (an "index entry").
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IdxeRecord {
    /// `file_offset / 16`.
    pub entry_id: u32,
    /// Raw type byte: low 5 bits = content type, 0x80 list, 0x40 has dupe id, 0x20 has sect/para.
    pub type_byte: u8,
    pub ref_id: i32,
    pub dupe_id: Option<i32>,
    pub sect_para: Option<u32>,
    pub word_count: u8,
    pub display_text: String,
    pub parents: Vec<i32>,
    /// Trailing u32 present on every record: a second `paraEnd<<16 | paraStart` range
    /// (0 when the record has no paragraph range).
    pub dual_para: u32,
}

impl IdxeRecord {
    pub fn content_type(&self) -> u8 {
        self.type_byte & 0x1F
    }
    pub fn is_list(&self) -> bool {
        self.type_byte & 0x80 != 0
    }
    /// `Some(section)` when the high half of `sect_para` is zero.
    pub fn section(&self) -> Option<u16> {
        self.sect_para.filter(|v| v >> 16 == 0).map(|v| v as u16)
    }
    /// `Some((para_start, para_end))` when the high half of `sect_para` is non-zero.
    pub fn para_range(&self) -> Option<(u16, u16)> {
        self.sect_para
            .filter(|v| v >> 16 != 0)
            .map(|v| (v as u16, (v >> 16) as u16))
    }
}

pub struct IdxE<'a> {
    pub data: &'a [u8],
    pub header: EffHeader,
}

impl<'a> IdxE<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let header = EffHeader::parse(data)?;
        ensure!(
            header.version[0] == 1,
            "unsupported IdXE version {:?}",
            header.version
        );
        Ok(IdxE { data, header })
    }

    /// Entry id of the first record slot after the header.
    pub fn first_entry_id(&self) -> u32 {
        self.header.end.div_ceil(16) as u32
    }

    /// Number of 16-byte slots in the file (upper bound on entry ids).
    pub fn slot_count(&self) -> u32 {
        self.data.len().div_ceil(16) as u32
    }

    /// Parses the record at `entry_id`; also returns the offset just past it.
    pub fn record_at(&self, entry_id: u32) -> Result<(IdxeRecord, usize)> {
        let off = entry_id as usize * 16;
        ensure!(
            off >= self.header.end && off < self.data.len(),
            "entry id {entry_id} outside the record area"
        );
        let mut c = Cursor::new(self.data, off);
        let type_byte = c.u8()?;
        let ref_id = c.i32()?;
        let dupe_id = if type_byte & 0x40 != 0 {
            Some(c.i32()?)
        } else {
            None
        };
        let sect_para = if type_byte & 0x20 != 0 {
            Some(c.u32()?)
        } else {
            None
        };
        let word_count = c.u8()?;
        let display_text = c.str_utf8()?;
        let n = c.u8()?;
        let parents = (0..n).map(|_| c.i32()).collect::<Result<_>>()?;
        let dual_para = c.u32()?;
        let rec = IdxeRecord {
            entry_id,
            type_byte,
            ref_id,
            dupe_id,
            sect_para,
            word_count,
            display_text,
            parents,
            dual_para,
        };
        Ok((rec, c.pos))
    }

    pub fn record(&self, entry_id: u32) -> Result<IdxeRecord> {
        Ok(self.record_at(entry_id)?.0)
    }

    /// Sequential walk: parse a record, continue at the next 16-byte boundary after it.
    /// This also reaches the few records no posting points to; a slot that does not parse
    /// is counted in `failures` and stepped over.
    pub fn walk(&self) -> IdxeWalk<'_, 'a> {
        IdxeWalk {
            idxe: self,
            next: self.first_entry_id(),
            failures: 0,
            last_end: self.header.end,
        }
    }
}

pub struct IdxeWalk<'r, 'a> {
    idxe: &'r IdxE<'a>,
    next: u32,
    /// Slots that did not parse and were stepped over.
    pub failures: u64,
    /// Offset just past the last record yielded (equals the file length on the disc files).
    pub last_end: usize,
}

impl Iterator for IdxeWalk<'_, '_> {
    type Item = IdxeRecord;
    fn next(&mut self) -> Option<IdxeRecord> {
        // Smallest record: type, refId, wordCount, length, nParents, dualPara = 12 bytes.
        while (self.next as usize) * 16 + 12 <= self.idxe.data.len() {
            match self.idxe.record_at(self.next) {
                Ok((rec, end)) => {
                    self.next = end.div_ceil(16) as u32;
                    self.last_end = end;
                    return Some(rec);
                }
                Err(_) => {
                    self.failures += 1;
                    self.next += 1;
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// IdxW

#[derive(Debug, Clone, Serialize)]
pub struct IdxwHeader {
    pub version: [u8; 3],
    pub encoding: String,
    pub last_update: u32,
    pub valid: u8,
    pub free_list_head: u32,
    pub iter: u32,
    pub max_keys_per_node: u16,
    pub key_factory: String,
    pub root_offset: u32,
}

/// One vocabulary key with its word entry.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WordKey {
    pub key: String,
    pub data_offset: u32,
    pub deleted: bool,
    /// Byte offset of the word's postings in IdXEW.
    pub word_id: u32,
    pub alt_id: u8,
    pub stop_word_type: u8,
}

impl WordKey {
    /// Word id 0 means "no postings" (observed only on stop words).
    pub fn has_postings(&self) -> bool {
        self.word_id != 0
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BTreeStats {
    pub nodes: u64,
    pub leaves: u64,
    pub depth: u32,
}

pub struct IdxW<'a> {
    pub data: &'a [u8],
    pub header: IdxwHeader,
}

impl<'a> IdxW<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let h = EffHeader::parse(data)?;
        let mut c = Cursor::new(data, h.end);
        let encoding = c.str_utf8()?;
        let last_update = c.u32()?;
        let valid = c.u8()?;
        let free_list_head = c.u32()?;
        let iter = c.u32()?;
        let max_keys_per_node = c.u16()?;
        let key_factory = c.str_utf8()?;
        let root_offset = c.u32()?;
        ensure!(encoding == "UTF8", "unsupported key encoding {encoding:?}");
        let header = IdxwHeader {
            version: h.version,
            encoding,
            last_update,
            valid,
            free_list_head,
            iter,
            max_keys_per_node,
            key_factory,
            root_offset,
        };
        Ok(IdxW { data, header })
    }

    /// In-order walk of the whole tree (child[0], key[0], child[1], …, key[n-1], child[n]).
    pub fn walk(&self) -> Result<(Vec<WordKey>, BTreeStats)> {
        let mut out = Vec::new();
        let mut stats = BTreeStats::default();
        self.walk_node(self.header.root_offset, 1, &mut out, &mut stats)?;
        Ok((out, stats))
    }

    fn walk_node(
        &self,
        off: u32,
        depth: u32,
        out: &mut Vec<WordKey>,
        stats: &mut BTreeStats,
    ) -> Result<()> {
        ensure!(depth <= 64, "B-tree deeper than 64 levels (cycle?)");
        stats.nodes += 1;
        stats.depth = stats.depth.max(depth);
        let mut c = Cursor::new(self.data, off as usize);
        let n = c.u16()? as usize;
        ensure!(
            n <= self.header.max_keys_per_node as usize,
            "node at {off:#x} has {n} keys"
        );
        let mut keys = Vec::with_capacity(n);
        for _ in 0..n {
            let data_offset = c.u32()?;
            let deleted = c.u8()? != 0;
            let len = c.u16()? as usize;
            let key =
                String::from_utf8(c.take(len)?.to_vec()).context("B-tree key is not UTF-8")?;
            let mut d = Cursor::new(self.data, data_offset as usize);
            let word_id = d.u32()?;
            let alt_id = d.u8()?;
            let stop_word_type = d.u8()?;
            keys.push(WordKey {
                key,
                data_offset,
                deleted,
                word_id,
                alt_id,
                stop_word_type,
            });
        }
        let nc = c.u16()? as usize;
        ensure!(
            nc == 0 || nc == n + 1,
            "node at {off:#x}: {n} keys but {nc} children"
        );
        let children: Vec<u32> = (0..nc).map(|_| c.u32()).collect::<Result<_>>()?;
        if children.is_empty() {
            stats.leaves += 1;
        }
        for (i, k) in keys.into_iter().enumerate() {
            if let Some(&ch) = children.get(i) {
                self.walk_node(ch, depth + 1, out, stats)?;
            }
            out.push(k);
        }
        if let Some(&ch) = children.last() {
            self.walk_node(ch, depth + 1, out, stats)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// IdXEW

/// The six bool flags after the IdXEW version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IdxewFlags {
    pub para_in_t3: bool,
    pub three_tiered: bool,
    pub short_rank: bool,
    pub para_in_t2: bool,
    pub skip_offset: bool,
    pub ref_id_in_t3: bool,
}

impl IdxewFlags {
    /// The only layout observed (and the only one this reader decodes).
    pub const OBSERVED: IdxewFlags = IdxewFlags {
        para_in_t3: true,
        three_tiered: true,
        short_rank: true,
        para_in_t2: true,
        skip_offset: false,
        ref_id_in_t3: true,
    };
}

/// Tier 1: the word is in the record's title.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct T1Hit {
    pub entry_id: u32,
    pub word_order: u8,
    pub rank: u16,
    pub ref_id: u32,
}

/// Tier 2: the word is in a heading or alias.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct T2Hit {
    pub entry_id: u32,
    pub rank: u16,
    /// refId with bit 31 cleared.
    pub ref_id: u32,
    /// Bit 31 of the stored refId.
    pub flag: bool,
    pub para_start: u16,
    pub para_end: u16,
}

/// Tier 3: body full-text hit.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct T3Hit {
    pub entry_id: u32,
    pub word_order: u8,
    pub ref_id: u32,
    pub flag: bool,
    pub para: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AltPostings {
    pub alt_id: u8,
    pub t1: Vec<T1Hit>,
    pub t2: Vec<T2Hit>,
    pub t3: Vec<T3Hit>,
}

impl AltPostings {
    pub fn entry_ids(&self) -> impl Iterator<Item = u32> + '_ {
        let a = self.t1.iter().map(|h| h.entry_id);
        let b = self.t2.iter().map(|h| h.entry_id);
        let c = self.t3.iter().map(|h| h.entry_id);
        a.chain(b).chain(c)
    }
}

pub struct IdxEw<'a> {
    pub data: &'a [u8],
    pub version: [u8; 3],
    pub flags: IdxewFlags,
    /// Offset of the first postings block.
    pub data_start: usize,
}

impl<'a> IdxEw<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let h = EffHeader::parse(data)?;
        let mut c = Cursor::new(data, h.end);
        let mut f = [false; 6];
        for x in &mut f {
            *x = c.u8()? != 0;
        }
        let flags = IdxewFlags {
            para_in_t3: f[0],
            three_tiered: f[1],
            short_rank: f[2],
            para_in_t2: f[3],
            skip_offset: f[4],
            ref_id_in_t3: f[5],
        };
        ensure!(
            flags == IdxewFlags::OBSERVED,
            "unsupported IdXEW flag combination {flags:?}"
        );
        Ok(IdxEw {
            data,
            version: h.version,
            flags,
            data_start: c.pos,
        })
    }

    /// Parses the postings block at `word_id`; also returns the offset just past it.
    /// Word id 0 (stop words) has no block and yields no postings.
    pub fn postings_at(&self, word_id: u32) -> Result<(Vec<AltPostings>, usize)> {
        if word_id == 0 {
            return Ok((Vec::new(), 0));
        }
        ensure!(
            word_id as usize >= self.data_start,
            "word id {word_id:#x} points into the header"
        );
        let mut c = Cursor::new(self.data, word_id as usize);
        let n_alt = c.u8()?;
        let mut alts = Vec::with_capacity(n_alt as usize);
        for _ in 0..n_alt {
            let alt_id = c.u8()?;
            let n1 = c.u16()?;
            let mut t1 = Vec::with_capacity(n1 as usize);
            for _ in 0..n1 {
                t1.push(T1Hit {
                    entry_id: c.u24()?,
                    word_order: c.u8()?,
                    rank: c.u16()?,
                    ref_id: c.u32()?,
                });
            }
            let n2 = c.u16()?;
            let mut t2 = Vec::with_capacity(n2 as usize);
            for _ in 0..n2 {
                let entry_id = c.u24()?;
                let rank = c.u16()?;
                let r = c.u32()?;
                let p = c.u32()?;
                t2.push(T2Hit {
                    entry_id,
                    rank,
                    ref_id: r & 0x7FFF_FFFF,
                    flag: r >> 31 != 0,
                    para_start: p as u16,
                    para_end: (p >> 16) as u16,
                });
            }
            let n3 = c.u16()?;
            let mut t3 = Vec::with_capacity(n3 as usize);
            for _ in 0..n3 {
                let entry_id = c.u24()?;
                let word_order = c.u8()?;
                let r = c.u32()?;
                let para = c.u16()?;
                t3.push(T3Hit {
                    entry_id,
                    word_order,
                    ref_id: r & 0x7FFF_FFFF,
                    flag: r >> 31 != 0,
                    para,
                });
            }
            alts.push(AltPostings { alt_id, t1, t2, t3 });
        }
        Ok((alts, c.pos))
    }

    pub fn postings(&self, word_id: u32) -> Result<Vec<AltPostings>> {
        Ok(self.postings_at(word_id)?.0)
    }
}

/// Distinct non-zero word ids (IdXEW offsets) referenced by the vocabulary, sorted.
pub fn distinct_word_ids(words: &[WordKey]) -> Vec<u32> {
    let mut ids: Vec<u32> = words
        .iter()
        .filter(|w| w.has_postings())
        .map(|w| w.word_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Every IdXE entry id referenced by any posting, sorted and de-duplicated.
/// These are the live records; the gaps between them hold stale data.
pub fn live_entry_ids(idxew: &IdxEw, word_ids: &[u32]) -> Result<Vec<u32>> {
    let mut seen: Vec<u64> = Vec::new();
    let mut mark = |id: u32| {
        let (w, b) = ((id / 64) as usize, id % 64);
        if w >= seen.len() {
            seen.resize(w + 1, 0);
        }
        seen[w] |= 1 << b;
    };
    for &w in word_ids {
        for alt in idxew
            .postings(w)
            .with_context(|| format!("postings at {w:#x}"))?
        {
            alt.entry_ids().for_each(&mut mark);
        }
    }
    let mut out = Vec::new();
    for (i, &bits) in seen.iter().enumerate() {
        for b in 0..64 {
            if bits >> b & 1 != 0 {
                out.push((i * 64 + b) as u32);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// IdXCM

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct CharMapEntry {
    pub ch: char,
    /// Search-normalised form (accent and case folded).
    pub normal: char,
    /// Form used when browsing ('\0' = none).
    pub browse: char,
}

pub struct CharMap {
    pub culture: String,
    pub answers: u8,
    pub entries: Vec<CharMapEntry>,
}

impl CharMap {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(b, 0);
        let culture = c.str_utf16()?;
        let answers = c.u8()?;
        let n = c.i32()?;
        ensure!(n >= 0, "negative char map count");
        let ch = |c: &mut Cursor| -> Result<char> {
            let u = c.u16()?;
            char::from_u32(u as u32).with_context(|| format!("surrogate {u:#x} in char map"))
        };
        let mut entries = Vec::with_capacity(n as usize);
        for _ in 0..n {
            entries.push(CharMapEntry {
                ch: ch(&mut c)?,
                normal: ch(&mut c)?,
                browse: ch(&mut c)?,
            });
        }
        ensure!(
            c.pos == b.len(),
            "{} trailing bytes after char map",
            b.len() - c.pos
        );
        Ok(CharMap {
            culture,
            answers,
            entries,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// File discovery

/// The files of one search set (`DLX` or `KDC`), found by name prefix so both the CAB names
/// (`IDXEDLX.EFF_E_A_09_XL_R_D`) and the installed names (`IDXEDLX.EFF`) work.
#[derive(Debug, Clone)]
pub struct FindFiles {
    pub pfx: PathBuf,
    pub idxe: PathBuf,
    pub idxw: PathBuf,
    pub idxew: PathBuf,
    pub idxcm: PathBuf,
}

impl FindFiles {
    pub fn locate(dir: &Path, tla: &str) -> Result<Self> {
        let names: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_ascii_uppercase(),
                    e.path(),
                )
            })
            .collect();
        let find = |stem: &str| -> Result<PathBuf> {
            let want = stem.to_ascii_uppercase();
            names
                .iter()
                .find(|(n, _)| *n == want || n.starts_with(&format!("{want}_")))
                .map(|(_, p)| p.clone())
                .with_context(|| format!("{stem} not found in {}", dir.display()))
        };
        Ok(FindFiles {
            pfx: find(&format!("PFX{tla}.DAT"))?,
            idxe: find(&format!("IDXE{tla}.EFF"))?,
            idxw: find(&format!("IDXW{tla}.EFF"))?,
            idxew: find(&format!("IDXEW{tla}.EFF"))?,
            idxcm: find(&format!("IDXCM{tla}.ECT"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn netstr(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut n = s.len();
        loop {
            let b = (n & 0x7F) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out.extend(s.as_bytes());
        out
    }

    fn eff_header(ver: [u8; 3]) -> Vec<u8> {
        let mut b = netstr("Copyright test header");
        b.extend(ver);
        b
    }

    fn pad16(b: &mut Vec<u8>) {
        while !b.len().is_multiple_of(16) {
            b.push(0);
        }
    }

    #[test]
    fn varint_and_strings() {
        let long = "x".repeat(300);
        let b = netstr(&long);
        assert_eq!(&b[..2], &[0xAC, 0x02]);
        let mut c = Cursor::new(&b, 0);
        assert_eq!(c.str_utf8().unwrap(), long);
        assert!(
            Cursor::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01], 0)
                .varint()
                .is_err()
        );
        assert!(Cursor::new(&[0x05, b'a'], 0).str_utf8().is_err());
    }

    #[test]
    fn pfx_two_collections() {
        let mut b = Vec::new();
        b.extend(2i32.to_le_bytes());
        b.extend(0x409u32.to_le_bytes());
        for (id, cls, s, d) in [
            (761000001u32, 1u16, "apple", "Apple"),
            (461000002, 203, "pear", "<it>Pear</it>"),
        ] {
            b.extend(id.to_le_bytes());
            b.extend(cls.to_le_bytes());
            b.extend(netstr(s));
            b.extend(netstr(d));
        }
        b.extend(1i32.to_le_bytes());
        b.extend(0x409u32.to_le_bytes());
        b.extend(7u32.to_le_bytes());
        b.extend(2214u16.to_le_bytes());
        b.extend(netstr("pie"));
        b.extend(netstr("Apple pie"));
        let p = Pfx::parse(&b).unwrap();
        assert_eq!(p.collections.len(), 2);
        assert_eq!(p.collections[0].entries[1].display_title, "<it>Pear</it>");
        assert_eq!(
            p.collections[1].entries[0],
            PfxEntry {
                ref_id: 7,
                class_id: 2214,
                sort_title: "pie".into(),
                display_title: "Apple pie".into()
            }
        );
        assert!(Pfx::parse(&b[..b.len() - 1]).is_err());
    }

    fn idxe_record(
        t: u8,
        ref_id: i32,
        extra: &[u32],
        wc: u8,
        text: &str,
        parents: &[i32],
        dual: u32,
    ) -> Vec<u8> {
        let mut b = vec![t];
        b.extend(ref_id.to_le_bytes());
        for x in extra {
            b.extend(x.to_le_bytes());
        }
        b.push(wc);
        b.extend(netstr(text));
        b.push(parents.len() as u8);
        for p in parents {
            b.extend(p.to_le_bytes());
        }
        b.extend(dual.to_le_bytes());
        b
    }

    /// Builds IdXE with records at known slots plus one record nothing points to.
    /// Returns (file, entry ids of the three posted records).
    fn sample_idxe() -> (Vec<u8>, Vec<u32>) {
        let mut b = eff_header([1, 6, 0]);
        pad16(&mut b);
        let mut ids = Vec::new();
        ids.push((b.len() / 16) as u32);
        b.extend(idxe_record(0x00, 761000001, &[], 1, "Zebra", &[], 0));
        pad16(&mut b);
        ids.push((b.len() / 16) as u32);
        b.extend(idxe_record(
            0x20,
            761000001,
            &[0x0005_0003],
            2,
            "Zebra : Habitat, a long section title",
            &[761000001],
            0x0004_0002,
        ));
        pad16(&mut b);
        b.extend(idxe_record(0x0C, 461000009, &[], 1, "unposted", &[], 0));
        pad16(&mut b);
        ids.push((b.len() / 16) as u32);
        b.extend(idxe_record(
            0x60 | 0x80,
            701000002,
            &[9, 0x0000_0004],
            1,
            "Equus",
            &[1, 2],
            0,
        ));
        pad16(&mut b);
        (b, ids)
    }

    #[test]
    fn idxe_records_and_walk() {
        let (b, ids) = sample_idxe();
        let f = IdxE::new(&b).unwrap();
        assert_eq!(f.first_entry_id(), ids[0]);
        let r = f.record(ids[1]).unwrap();
        assert_eq!(r.ref_id, 761000001);
        assert_eq!(r.para_range(), Some((3, 5)));
        assert_eq!(r.dual_para, 0x0004_0002);
        assert_eq!(r.parents, vec![761000001]);
        let r = f.record(ids[2]).unwrap();
        assert_eq!(
            (r.content_type(), r.is_list(), r.dupe_id, r.section()),
            (0, true, Some(9), Some(4))
        );
        assert_eq!(r.parents, vec![1, 2]);
        let walked: Vec<_> = f.walk().collect();
        assert_eq!(walked.len(), 4);
        assert_eq!(walked[2].display_text, "unposted");
        assert_eq!(walked[3].entry_id, ids[2]);
        let mut w = f.walk();
        w.by_ref().count();
        assert_eq!((w.failures, w.last_end.div_ceil(16)), (0, b.len() / 16));
    }

    #[test]
    fn idxe_record_ending_on_boundary() {
        // 1+4+1+1+5+1+4 = 17 bytes would spill; 1+4+1+1+4+1+4 = 16 bytes fits one slot exactly.
        let mut b = eff_header([1, 6, 0]);
        pad16(&mut b);
        let first = (b.len() / 16) as u32;
        b.extend(idxe_record(0, 5, &[], 1, "abcd", &[], 0));
        assert_eq!(b.len() % 16, 0);
        b.extend(idxe_record(0, 6, &[], 1, "next", &[], 0));
        pad16(&mut b);
        let f = IdxE::new(&b).unwrap();
        let ids: Vec<u32> = f.walk().map(|r| r.entry_id).collect();
        assert_eq!(ids, vec![first, first + 1]);
    }

    /// Builds IdxW with a two-level tree: root ["m"], children ["a","c"] and ["x"].
    fn sample_idxw() -> Vec<u8> {
        let mut b = eff_header([1, 8, 0]);
        b.extend(netstr("UTF8"));
        b.extend(0u32.to_le_bytes());
        b.push(1);
        b.extend(0u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(201u16.to_le_bytes());
        b.extend(netstr("Test.KeyFactory"));
        let root_field = b.len();
        b.extend(0u32.to_le_bytes());
        // word entries
        let mut data = std::collections::HashMap::new();
        for (k, wid) in [
            ("a", 100u32),
            ("c", 200),
            ("m", 300),
            ("x", 300),
            ("the", 0),
        ] {
            data.insert(k, b.len() as u32);
            b.extend(wid.to_le_bytes());
            b.push(if k == "x" { 1 } else { 0 });
            b.push(0);
        }
        let node = |b: &mut Vec<u8>, keys: &[&str], children: &[u32]| -> u32 {
            let off = b.len() as u32;
            b.extend((keys.len() as u16).to_le_bytes());
            for k in keys {
                b.extend(data[k].to_le_bytes());
                b.push(0);
                b.extend((k.len() as u16).to_le_bytes());
                b.extend(k.as_bytes());
            }
            b.extend((children.len() as u16).to_le_bytes());
            for c in children {
                b.extend(c.to_le_bytes());
            }
            off
        };
        let l1 = node(&mut b, &["a", "c"], &[]);
        let l2 = node(&mut b, &["the", "x"], &[]);
        let root = node(&mut b, &["m"], &[l1, l2]);
        b[root_field..root_field + 4].copy_from_slice(&root.to_le_bytes());
        b
    }

    #[test]
    fn idxw_in_order_walk() {
        let b = sample_idxw();
        let w = IdxW::new(&b).unwrap();
        assert_eq!(w.header.max_keys_per_node, 201);
        let (keys, stats) = w.walk().unwrap();
        assert_eq!(
            keys.iter().map(|k| k.key.as_str()).collect::<Vec<_>>(),
            ["a", "c", "m", "the", "x"]
        );
        assert_eq!((stats.nodes, stats.leaves, stats.depth), (3, 2, 2));
        assert_eq!(keys[4].alt_id, 1);
        assert!(!keys[3].has_postings());
        assert_eq!(distinct_word_ids(&keys), vec![100, 200, 300]);
    }

    #[test]
    fn idxew_postings() {
        let mut b = eff_header([1, 11, 0]);
        b.extend([1, 1, 1, 1, 0, 1]);
        let w = b.len() as u32;
        b.push(1); // one alt
        b.push(0); // alt id
        b.extend(1u16.to_le_bytes());
        b.extend([0x34, 0x12, 0x01, 2]); // entry 0x011234, word order 2
        b.extend(500u16.to_le_bytes());
        b.extend(761000001u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend([5, 0, 0]);
        b.extend(9u16.to_le_bytes());
        b.extend((761000001u32 | 1 << 31).to_le_bytes());
        b.extend((7u32 << 16 | 3).to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend([6, 0, 0, 4]);
        b.extend(761000002u32.to_le_bytes());
        b.extend(12u16.to_le_bytes());
        let f = IdxEw::new(&b).unwrap();
        assert_eq!(f.data_start, w as usize);
        let (alts, end) = f.postings_at(w).unwrap();
        assert_eq!(end, b.len());
        let a = &alts[0];
        assert_eq!(
            a.t1[0],
            T1Hit {
                entry_id: 0x011234,
                word_order: 2,
                rank: 500,
                ref_id: 761000001
            }
        );
        assert_eq!(
            a.t2[0],
            T2Hit {
                entry_id: 5,
                rank: 9,
                ref_id: 761000001,
                flag: true,
                para_start: 3,
                para_end: 7
            }
        );
        assert_eq!(
            a.t3[0],
            T3Hit {
                entry_id: 6,
                word_order: 4,
                ref_id: 761000002,
                flag: false,
                para: 12
            }
        );
        assert_eq!(live_entry_ids(&f, &[w]).unwrap(), vec![5, 6, 0x011234]);
        assert!(f.postings(0).unwrap().is_empty());
        assert!(f.postings(3).is_err());
        let mut bad = b.clone();
        bad[eff_header([1, 11, 0]).len() + 4] = 1; // skipOffset set
        assert!(IdxEw::new(&bad).is_err());
    }

    #[test]
    fn char_map() {
        let mut b = vec![10];
        b.extend("en-US".encode_utf16().flat_map(u16::to_le_bytes));
        b.push(0);
        b.extend(2i32.to_le_bytes());
        for (c, n, br) in [('\u{d3}', 'O', '\0'), ('s', 's', 'S')] {
            for x in [c, n, br] {
                b.extend((x as u16).to_le_bytes());
            }
        }
        let m = CharMap::parse(&b).unwrap();
        assert_eq!(m.culture, "en-US");
        assert_eq!(
            m.entries[0],
            CharMapEntry {
                ch: 'Ó',
                normal: 'O',
                browse: '\0'
            }
        );
        assert_eq!(m.entries[1].browse, 'S');
        b.push(0);
        assert!(CharMap::parse(&b).is_err());
    }
}
