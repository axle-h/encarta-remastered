//! Reader for `.AKC` article/data packages (`CONT*`, `DATA*`, `DATAF*`, `REL*`, Encarta and Kids).
//!
//! Spec: `docs/formats/akc.md`. A package holds one shared dictionary, a record index and
//! compressed blocks; each block carries its own dictionary and a Huffman/LZ77 token stream
//! that an XML emitter turns into record text. Several records usually share one block, so
//! decoded blocks are cached ([`AkcFile::get`]) and bulk decoding works block by block
//! ([`AkcFile::for_each_record`], parallel over blocks).

mod bits;
pub mod check;
mod dict;
mod emit;
mod huffman;
mod index;
pub mod library;
mod tokens;

pub use dict::{BlockDict, CodeStats, DictHeader, Kind, SharedDict};
pub use index::{Field, IndexHeader};
pub use library::Library;
pub use tokens::TokenStats;

use anyhow::{Context, Result, bail, ensure};
use bits::BitReader;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MAGIC: u32 = 0x414B_4320;
const HEADER_LEN: usize = 0x30;
/// Decoded blocks kept by [`AkcFile::get`].
const CACHE_BLOCKS: usize = 8;

/// The fixed file header (§2).
#[derive(Debug, Clone)]
pub struct Header {
    pub version: u32,
    pub guid: [u8; 16],
    pub flags: u32,
    pub dict_offset: u32,
    pub dict_len: u32,
    pub index_offset: u32,
    pub index_len: u32,
    /// Build label (UTF-16LE XML in the file), BOM removed.
    pub label: String,
}

impl Header {
    /// Stub packages carry no dictionary and no index.
    pub fn is_stub(&self) -> bool {
        self.flags & 1 != 0
    }

    /// The package GUID in registry form, `{0EA8A288-…}`, as used by the catalog.
    pub fn guid_string(&self) -> String {
        let g = &self.guid;
        let d1 = u32::from_le_bytes(g[0..4].try_into().unwrap());
        let d2 = u16::from_le_bytes([g[4], g[5]]);
        let d3 = u16::from_le_bytes([g[6], g[7]]);
        let tail: String = g[8..].iter().map(|b| format!("{b:02X}")).collect();
        format!(
            "{{{d1:08X}-{d2:04X}-{d3:04X}-{}-{}}}",
            &tail[..4],
            &tail[4..]
        )
    }

    pub fn parse(b: &[u8]) -> Result<Header> {
        ensure!(b.len() >= HEADER_LEN, "file too short for an AKC header");
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        ensure!(u(0) == MAGIC, "bad magic {:#010x}", u(0));
        ensure!(u(4) == 1, "unsupported AKC version {}", u(4));
        let label_len = u(0x2C) as usize;
        let raw = b
            .get(HEADER_LEN..HEADER_LEN + label_len)
            .context("label runs past end of file")?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        let mut label = String::from_utf16_lossy(&units);
        if label.starts_with('\u{FEFF}') {
            label.remove(0);
        }
        Ok(Header {
            version: u(4),
            guid: b[8..24].try_into().unwrap(),
            flags: u(0x18),
            dict_offset: u(0x1C),
            dict_len: u(0x20),
            index_offset: u(0x24),
            index_len: u(0x28),
            label,
        })
    }
}

/// One record, located by the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordInfo {
    /// Local record number (the catalog's local id).
    pub key: u32,
    /// Index into [`AkcFile::blocks`].
    pub block: u32,
    /// First token of the record within the block's token stream.
    pub start: u32,
    pub ntok: u32,
    /// Decoded length in bytes.
    pub ulen: u32,
}

/// One compressed block and the records that live in it.
#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub offset: u64,
    pub clen: u32,
    /// The block belongs to a single "direct" record (§3).
    pub direct: bool,
    /// Tokens needed to decode every member (largest `start + ntok`).
    pub tokens: u32,
    /// Record positions (indexes into [`AkcFile::records`]), by increasing `start`.
    pub members: Vec<u32>,
}

/// A block's dictionary and its decoded token stream.
#[derive(Debug)]
pub struct DecodedBlock {
    pub dict: BlockDict,
    pub tokens: Vec<u32>,
    pub stats: TokenStats,
    /// Bits left in the block after the last needed token, and whether they are all zero.
    pub trailing_bits: (u64, bool),
}

enum Source {
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl std::ops::Deref for Source {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Source::Mapped(m) => m,
            Source::Owned(v) => v,
        }
    }
}

/// An open AKC package.
pub struct AkcFile {
    data: Source,
    path: Option<PathBuf>,
    header: Header,
    shared: Option<SharedDict>,
    index_header: Option<IndexHeader>,
    block_entries: usize,
    sparse_entries: usize,
    records: Vec<RecordInfo>,
    blocks: Vec<BlockInfo>,
    /// Keys are dense (record `i` has key `i`), so lookup is direct.
    dense: bool,
    cache: Mutex<Vec<(u32, Arc<DecodedBlock>)>>,
}

fn slice(data: &[u8], off: u64, len: u64, what: &str) -> Result<std::ops::Range<usize>> {
    let end = off.checked_add(len).context("offset overflow")?;
    ensure!(
        end <= data.len() as u64,
        "{what} ({off}+{len}) runs past end of file ({})",
        data.len()
    );
    Ok(off as usize..end as usize)
}

impl AkcFile {
    /// Memory-maps and opens `path`: parses the header and index and decodes the shared dictionary.
    pub fn open(path: impl AsRef<Path>) -> Result<AkcFile> {
        let path = path.as_ref();
        let f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        // SAFETY: the disc files are read-only inputs; nothing truncates them while mapped.
        let map = unsafe { memmap2::Mmap::map(&f) }
            .with_context(|| format!("mapping {}", path.display()))?;
        let mut a = Self::from_source(Source::Mapped(map))
            .with_context(|| format!("reading {}", path.display()))?;
        a.path = Some(path.to_path_buf());
        Ok(a)
    }

    /// Opens a package held in memory.
    pub fn from_bytes(data: Vec<u8>) -> Result<AkcFile> {
        Self::from_source(Source::Owned(data))
    }

    fn from_source(data: Source) -> Result<AkcFile> {
        let header = Header::parse(&data)?;
        let mut a = AkcFile {
            data,
            path: None,
            header,
            shared: None,
            index_header: None,
            block_entries: 0,
            sparse_entries: 0,
            records: Vec::new(),
            blocks: Vec::new(),
            dense: true,
            cache: Mutex::new(Vec::new()),
        };
        if a.header.is_stub() {
            return Ok(a);
        }
        let h = &a.header;
        let d = slice(
            &a.data,
            h.dict_offset as u64,
            h.dict_len as u64,
            "dictionary",
        )?;
        let shared = SharedDict::parse(&a.data[d]).context("shared dictionary")?;
        let i = slice(&a.data, h.index_offset as u64, h.index_len as u64, "index")?;
        let index = index::Index::parse(&a.data[i]).context("record index")?;

        let mut by_offset: HashMap<u64, u32> = HashMap::new();
        let mut prev_key: Option<u32> = None;
        for (pos, r) in index.records.iter().enumerate() {
            if let Some(p) = prev_key {
                ensure!(r.key > p, "record keys not ascending ({p} then {})", r.key);
            }
            prev_key = Some(r.key);
            a.dense &= r.key as usize == pos;
            let fits = |v: u64| u32::try_from(v).ok();
            let (Some(clen), Some(ulen), Some(ntok), Some(start)) =
                (fits(r.clen), fits(r.ulen), fits(r.ntok), fits(r.start))
            else {
                bail!("record {}: field out of range", r.key);
            };
            // Every token emits at least one byte.
            ensure!(
                ntok <= ulen,
                "record {}: {ntok} tokens for {ulen} bytes",
                r.key
            );
            let end = start
                .checked_add(ntok)
                .with_context(|| format!("record {}: token range overflow", r.key))?;
            slice(&a.data, r.offset, r.clen, "block")
                .with_context(|| format!("record {}", r.key))?;
            let block = if r.direct {
                None
            } else {
                by_offset.get(&r.offset).copied()
            };
            let block = match block {
                Some(b) => {
                    let bi = &mut a.blocks[b as usize];
                    ensure!(
                        bi.clen == clen && !bi.direct,
                        "record {}: block at {} with inconsistent length",
                        r.key,
                        r.offset
                    );
                    bi.tokens = bi.tokens.max(end);
                    bi.members.push(pos as u32);
                    b
                }
                None => {
                    let b = a.blocks.len() as u32;
                    a.blocks.push(BlockInfo {
                        offset: r.offset,
                        clen,
                        direct: r.direct,
                        tokens: end,
                        members: vec![pos as u32],
                    });
                    if !r.direct {
                        by_offset.insert(r.offset, b);
                    }
                    b
                }
            };
            a.records.push(RecordInfo {
                key: r.key,
                block,
                start,
                ntok,
                ulen,
            });
        }
        let recs = &a.records;
        for b in &mut a.blocks {
            b.members.sort_by_key(|&m| recs[m as usize].start);
        }
        a.shared = Some(shared);
        a.index_header = Some(index.header);
        a.block_entries = index.block_entries;
        a.sparse_entries = index.groups.len();
        Ok(a)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The shared dictionary (`None` for stub packages).
    pub fn shared(&self) -> Option<&SharedDict> {
        self.shared.as_ref()
    }

    pub fn index_header(&self) -> Option<&IndexHeader> {
        self.index_header.as_ref()
    }

    /// Number of block entries `M` in the index.
    pub fn block_entries(&self) -> usize {
        self.block_entries
    }

    /// Number of sparse-table entries (`G + 1`).
    pub fn sparse_entries(&self) -> usize {
        self.sparse_entries
    }

    /// Records in key order.
    pub fn records(&self) -> &[RecordInfo] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn blocks(&self) -> &[BlockInfo] {
        &self.blocks
    }

    /// Position of `key` in [`AkcFile::records`].
    pub fn position(&self, key: u32) -> Option<usize> {
        if self.dense {
            return ((key as usize) < self.records.len()).then_some(key as usize);
        }
        self.records.binary_search_by_key(&key, |r| r.key).ok()
    }

    pub fn record_info(&self, key: u32) -> Option<&RecordInfo> {
        self.position(key).map(|p| &self.records[p])
    }

    /// The raw bytes of a compressed block.
    pub fn block_bytes(&self, block: u32) -> &[u8] {
        let b = &self.blocks[block as usize];
        &self.data[b.offset as usize..b.offset as usize + b.clen as usize]
    }

    /// Decodes a block (its dictionary and every token any member needs), bypassing the cache.
    pub fn decode_block(&self, block: u32) -> Result<DecodedBlock> {
        let shared = self.shared.as_ref().context("stub package has no blocks")?;
        let info = self
            .blocks
            .get(block as usize)
            .with_context(|| format!("no block {block}"))?;
        let run = || -> Result<DecodedBlock> {
            let mut r = BitReader::new(self.block_bytes(block))?;
            let dict = BlockDict::parse(&mut r, shared)?;
            let code = dict.token_code.as_ref().unwrap_or(&shared.token_code);
            let (tokens, stats) = tokens::decode_tokens(
                &mut r,
                code,
                dict.words.len() as u32,
                shared.words.len() as u32,
                info.tokens as usize,
            )?;
            Ok(DecodedBlock {
                trailing_bits: r.trailing(),
                dict,
                tokens,
                stats,
            })
        };
        run().with_context(|| format!("block {block} at offset {}", info.offset))
    }

    /// Decodes a block through the small most-recently-used cache.
    pub fn cached_block(&self, block: u32) -> Result<Arc<DecodedBlock>> {
        {
            let mut c = self.cache.lock().unwrap();
            if let Some(i) = c.iter().position(|(b, _)| *b == block) {
                let e = c.remove(i);
                let d = e.1.clone();
                c.push(e);
                return Ok(d);
            }
        }
        let d = Arc::new(self.decode_block(block)?);
        let mut c = self.cache.lock().unwrap();
        if c.len() >= CACHE_BLOCKS {
            c.remove(0);
        }
        c.push((block, d.clone()));
        Ok(d)
    }

    /// Renders one record from its decoded block.
    pub fn render(&self, block: &DecodedBlock, rec: &RecordInfo) -> Result<Vec<u8>> {
        let shared = self.shared.as_ref().context("stub package")?;
        let s = rec.start as usize;
        let tokens = block
            .tokens
            .get(s..s + rec.ntok as usize)
            .context("record token range beyond decoded tokens")?;
        emit::emit(
            tokens,
            &block.dict.words,
            &shared.words,
            emit::EmitFlags {
                capitalize: shared.capitalize,
                self_closing: shared.self_closing,
            },
            rec.ulen as usize,
        )
        .with_context(|| format!("record {}", rec.key))
    }

    /// Decodes the record with local id `key` (`None` if the package has no such record).
    pub fn get(&self, key: u32) -> Result<Option<Vec<u8>>> {
        let Some(rec) = self.record_info(key) else {
            return Ok(None);
        };
        let block = self.cached_block(rec.block)?;
        self.render(&block, rec).map(Some)
    }

    /// Like [`AkcFile::get`], failing when the record does not exist.
    pub fn record(&self, key: u32) -> Result<Vec<u8>> {
        self.get(key)?
            .with_context(|| format!("no record {key} in package"))
    }

    /// Decodes every record, in parallel over blocks. `f` receives the record's position in
    /// [`AkcFile::records`], its info and its bytes; records of one block arrive in token order.
    pub fn for_each_record<F>(&self, f: F) -> Result<()>
    where
        F: Fn(usize, &RecordInfo, &[u8]) -> Result<()> + Sync,
    {
        (0..self.blocks.len() as u32)
            .into_par_iter()
            .try_for_each(|b| {
                let d = self.decode_block(b)?;
                for &m in &self.blocks[b as usize].members {
                    let rec = &self.records[m as usize];
                    f(m as usize, rec, &self.render(&d, rec)?)?;
                }
                Ok(())
            })
    }

    /// Sequential iterator over all records as `(key, bytes)`, in block order (each block decoded once).
    pub fn iter(&self) -> impl Iterator<Item = Result<(u32, Vec<u8>)>> + '_ {
        self.blocks.iter().enumerate().flat_map(move |(b, info)| {
            let d = self.decode_block(b as u32).map(Arc::new);
            info.members.iter().map(move |&m| {
                let rec = &self.records[m as usize];
                match &d {
                    Ok(d) => self.render(d, rec).map(|x| (rec.key, x)),
                    Err(e) => Err(anyhow::anyhow!("{e:#}")),
                }
            })
        })
    }
}

/// The record text without its UTF-8 BOM, if valid UTF-8 (records are UTF-8, §12).
pub fn xml_text(bytes: &[u8]) -> Result<&str> {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    std::str::from_utf8(b).context("record is not valid UTF-8")
}

#[cfg(test)]
mod tests;
