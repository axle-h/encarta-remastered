//! Reader for `ITOLITLS` containers (Encarta `.EIT`, `CATALOG.DLX`, `MSWORLD.ITR`).
//!
//! The layout is the one used by Microsoft Reader `.lit` files: a header with
//! "pieces", an `IFCM`/`AOLL` directory of `(name, section, offset, size)`
//! entries, and named sections whose content may be LZX-compressed.

use anyhow::{Context, Result, bail, ensure};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const LZX_GUID: &str = "{0A9007C6-4076-11D3-8789-0000F8105754}";

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub section: u32,
    pub offset: u64,
    pub size: u64,
}

pub struct Itol {
    file: File,
    pub content_offset: u64,
    pub entries: Vec<Entry>,
    index: HashMap<String, usize>,
    pub sections: Vec<String>,
    section_cache: HashMap<u32, Vec<u8>>,
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

/// Big-endian base-128 integer, high bit = continuation.
fn encint(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut v: u64 = 0;
    loop {
        let byte = *b.get(*pos).context("encint past end")?;
        *pos += 1;
        v = (v << 7) | (byte & 0x7f) as u64;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
}

pub fn guid_string(b: &[u8]) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        u32le(b, 0),
        u16le(b, 4),
        u16le(b, 6),
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

impl Itol {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut hdr = [0u8; 0x28];
        file.read_exact(&mut hdr)?;
        ensure!(&hdr[0..8] == b"ITOLITLS", "not an ITOLITLS file");
        let hdr_len = u32le(&hdr, 0x0c) as u64;
        let num_pieces = u32le(&hdr, 0x10) as u64;
        let sec_hdr_len = u32le(&hdr, 0x14) as usize;

        let mut pieces_raw = vec![0u8; (num_pieces * 16) as usize];
        file.seek(SeekFrom::Start(hdr_len))?;
        file.read_exact(&mut pieces_raw)?;
        let pieces: Vec<(u64, u64)> = (0..num_pieces as usize)
            .map(|i| (u64le(&pieces_raw, i * 16), u64le(&pieces_raw, i * 16 + 8)))
            .collect();

        let mut sec = vec![0u8; sec_hdr_len];
        file.read_exact(&mut sec)?;
        let mut content_offset = None;
        let mut p = u32le(&sec, 4) as usize;
        while p + 8 <= sec.len() {
            match &sec[p..p + 4] {
                b"CAOL" => p += 48,
                b"ITSF" => {
                    content_offset = Some(u64le(&sec, p + 16));
                    p += 48;
                }
                _ => break,
            }
        }
        let content_offset = content_offset.context("no ITSF block in secondary header")?;

        let (dir_off, dir_len) = pieces[1];
        let mut dir = vec![0u8; dir_len as usize];
        file.seek(SeekFrom::Start(dir_off))?;
        file.read_exact(&mut dir)?;
        let entries = parse_directory(&dir)?;
        let index = entries.iter().enumerate().map(|(i, e)| (e.name.clone(), i)).collect();

        let mut me = Itol {
            file,
            content_offset,
            entries,
            index,
            sections: vec![],
            section_cache: HashMap::new(),
        };
        me.sections = me.read_namelist()?;
        Ok(me)
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.index.get(name).map(|&i| &self.entries[i])
    }

    fn read_raw(&mut self, off: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(off))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read an entry stored in the uncompressed section 0.
    fn read_uncompressed(&mut self, name: &str) -> Result<Vec<u8>> {
        let e = self.entry(name).with_context(|| format!("missing {name}"))?.clone();
        ensure!(e.section == 0, "{name} is not in section 0");
        self.read_raw(self.content_offset + e.offset, e.size)
    }

    fn read_namelist(&mut self) -> Result<Vec<String>> {
        let b = self.read_uncompressed("::DataSpace/NameList")?;
        let n = u16le(&b, 2) as usize;
        let mut p = 4;
        let mut out = vec![];
        for _ in 0..n {
            let len = u16le(&b, p) as usize;
            p += 2;
            let units: Vec<u16> = (0..len).map(|i| u16le(&b, p + i * 2)).collect();
            out.push(String::from_utf16_lossy(&units));
            p += (len + 1) * 2;
        }
        Ok(out)
    }

    /// Raw storage streams for a section, for inspection.
    pub fn section_storage(&mut self, section: &str, stream: &str) -> Result<Vec<u8>> {
        self.read_uncompressed(&format!("::DataSpace/Storage/{section}/{stream}"))
    }

    pub fn transforms(&mut self, section: &str) -> Result<Vec<String>> {
        let b = self.section_storage(section, "Transform/List")?;
        Ok(b.chunks_exact(16).map(guid_string).collect())
    }

    fn section_data(&mut self, idx: u32) -> Result<&[u8]> {
        if !self.section_cache.contains_key(&idx) {
            let name = self.sections[idx as usize].clone();
            let content = self.section_storage(&name, "Content")?;
            let data = match self.transforms(&name)?.as_slice() {
                [] => content,
                [g] if g == LZX_GUID => {
                    let control = self.section_storage(&name, "ControlData")?;
                    let reset = self.section_storage(
                        &name,
                        &format!("Transform/{LZX_GUID}/InstanceData/ResetTable"),
                    )?;
                    lzx_decompress(&content, &control, &reset)
                        .with_context(|| format!("decompressing section {name}"))?
                }
                other => bail!("unsupported transforms {other:?} on section {name}"),
            };
            self.section_cache.insert(idx, data);
        }
        Ok(&self.section_cache[&idx])
    }

    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let e = self.entry(name).with_context(|| format!("missing {name}"))?.clone();
        if e.section == 0 {
            return self.read_raw(self.content_offset + e.offset, e.size);
        }
        let data = self.section_data(e.section)?;
        let end = (e.offset + e.size) as usize;
        ensure!(end <= data.len(), "{name} extends past section end");
        Ok(data[e.offset as usize..end].to_vec())
    }
}

fn parse_directory(dir: &[u8]) -> Result<Vec<Entry>> {
    ensure!(&dir[0..4] == b"IFCM", "directory missing IFCM");
    let chunk_size = u32le(dir, 8) as usize;
    let num_chunks = u32le(dir, 0x18) as usize;
    let mut entries = vec![];
    for c in 0..num_chunks {
        let chunk = &dir[0x20 + c * chunk_size..0x20 + (c + 1) * chunk_size];
        if &chunk[0..4] != b"AOLL" {
            continue;
        }
        let free = u32le(chunk, 4) as usize;
        let end = chunk_size - free;
        let mut p = 0x30;
        while p < end {
            let len = encint(chunk, &mut p)? as usize;
            let name = String::from_utf8_lossy(&chunk[p..p + len]).into_owned();
            p += len;
            let section = encint(chunk, &mut p)? as u32;
            let offset = encint(chunk, &mut p)?;
            let size = encint(chunk, &mut p)?;
            entries.push(Entry { name, section, offset, size });
        }
    }
    Ok(entries)
}

/// LZX as used by CHM/LIT: ResetTable gives the compressed offset of each
/// 32 KiB output frame; the decoder state is reset every `reset_interval` frames.
fn lzx_decompress(content: &[u8], control: &[u8], reset: &[u8]) -> Result<Vec<u8>> {
    ensure!(&control[4..8] == b"LZXC", "control data is not LZXC");
    let reset_interval = u32le(control, 12) as usize;
    let window_units = u32le(control, 16) as usize; // window size in 32 KiB units
    let window = match window_units * 0x8000 {
        0x8000 => lzxd::WindowSize::KB32,
        0x10000 => lzxd::WindowSize::KB64,
        0x20000 => lzxd::WindowSize::KB128,
        0x40000 => lzxd::WindowSize::KB256,
        0x80000 => lzxd::WindowSize::KB512,
        0x100000 => lzxd::WindowSize::MB1,
        0x200000 => lzxd::WindowSize::MB2,
        w => bail!("unsupported LZX window {w:#x}"),
    };

    let n = u32le(reset, 4) as usize;
    let hdr_len = u32le(reset, 12) as usize;
    let ulen = u64le(reset, 16) as usize;
    let clen = u64le(reset, 24) as usize;
    let block = u64le(reset, 32) as usize;
    ensure!(block == 0x8000, "unexpected LZX frame size {block:#x}");
    let offs: Vec<usize> = (0..n).map(|i| u64le(reset, hdr_len + i * 8) as usize).collect();
    let frames_per_reset = (reset_interval * 0x8000 / block).max(1);

    let frame_input = |i: usize| {
        let end = offs.get(i + 1).copied().unwrap_or(clen.min(content.len()));
        &content[offs[i]..end]
    };
    // Frames between reset points share decoder state, so decode a group at a time.
    let decode_group = |first: usize, last: usize, last_len: usize| -> Result<Vec<u8>> {
        let mut lzx = lzxd::Lzxd::new(window);
        let mut out = vec![];
        for i in first..=last {
            let want = if i == last { last_len } else { block };
            let frame = lzx
                .decompress_next(frame_input(i), want)
                .map_err(|e| anyhow::anyhow!("frame {i}: {e:?}"))?;
            out.extend_from_slice(frame);
        }
        Ok(out)
    };

    let mut out = Vec::with_capacity(ulen);
    for first in (0..n).step_by(frames_per_reset) {
        let last = (first + frames_per_reset - 1).min(n - 1);
        let last_len = block.min(ulen - last * block);
        let group = match decode_group(first, last, last_len) {
            Ok(g) => g,
            // Some streams pad the final frame to a full 32 KiB and end it
            // with a match that runs past the real data; lzxd only stops
            // exactly on the requested length, so decode the padding too.
            Err(_) if last == n - 1 && last_len < block => {
                let mut g = decode_group(first, last, block)?;
                g.truncate(g.len() - (block - last_len));
                g
            }
            Err(e) => return Err(e),
        };
        out.extend_from_slice(&group);
    }
    ensure!(out.len() == ulen, "decompressed {} of {} bytes", out.len(), ulen);
    Ok(out)
}
