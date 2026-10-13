//! PMTiles v3 archives: a writer that streams tile data to a scratch file and a small reader used
//! for validation and tests. Layout per the public PMTiles v3 specification: a 127-byte header,
//! a gzip-compressed root directory, gzip-compressed JSON metadata, leaf directories, tile data.
//! Tile ids follow the Hilbert curve of each zoom level; identical small tiles are stored once.

use anyhow::{Context, Result, bail, ensure};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

const HEADER_LEN: usize = 127;
/// Header plus root directory must fit in the first 16 KiB.
const ROOT_LIMIT: usize = 16_384 - HEADER_LEN;
/// Tiles up to this size are deduplicated by content.
const DEDUP_MAX: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Mvt = 1,
    Png = 2,
    Webp = 4,
}

/// Hilbert tile id of `(z, x, y)`.
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    let acc = ((1u64 << (2 * z as u64)) - 1) / 3;
    let n = 1u64 << z;
    let (mut x, mut y) = (x as u64, y as u64);
    let mut d = 0u64;
    let mut s = n / 2;
    while s > 0 {
        let rx = u64::from(x & s > 0);
        let ry = u64::from(y & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        if ry == 0 {
            if rx == 1 {
                x = n - 1 - x;
                y = n - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    acc + d
}

/// Inverse of [`tile_id`].
pub fn tile_zxy(id: u64) -> (u8, u32, u32) {
    let mut z = 0u8;
    let mut acc = 0u64;
    loop {
        let count = 1u64 << (2 * z as u64);
        if id < acc + count {
            break;
        }
        acc += count;
        z += 1;
    }
    let n = 1u64 << z;
    let mut t = id - acc;
    let (mut x, mut y) = (0u64, 0u64);
    let mut s = 1u64;
    while s < n {
        let rx = 1 & (t / 2);
        let ry = 1 & (t ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        t /= 4;
        s *= 2;
    }
    (z, x as u32, y as u32)
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn read_varint(b: &[u8], pos: &mut usize) -> Result<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let byte = *b.get(*pos).context("truncated varint")?;
        *pos += 1;
        v |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        ensure!(shift < 64, "varint too long");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub tile_id: u64,
    pub offset: u64,
    pub length: u32,
    /// 0 marks a leaf-directory pointer.
    pub run_length: u32,
}

fn gzip(b: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(b).expect("in-memory gzip");
    e.finish().expect("in-memory gzip")
}

fn gunzip(b: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    GzDecoder::new(b).read_to_end(&mut out)?;
    Ok(out)
}

pub fn serialize_directory(entries: &[Entry]) -> Vec<u8> {
    let mut b = Vec::new();
    varint(&mut b, entries.len() as u64);
    let mut last = 0u64;
    for e in entries {
        varint(&mut b, e.tile_id - last);
        last = e.tile_id;
    }
    for e in entries {
        varint(&mut b, e.run_length as u64);
    }
    for e in entries {
        varint(&mut b, e.length as u64);
    }
    for (i, e) in entries.iter().enumerate() {
        if i > 0 && e.offset == entries[i - 1].offset + entries[i - 1].length as u64 {
            varint(&mut b, 0);
        } else {
            varint(&mut b, e.offset + 1);
        }
    }
    gzip(&b)
}

pub fn parse_directory(compressed: &[u8]) -> Result<Vec<Entry>> {
    let b = gunzip(compressed)?;
    let mut p = 0;
    let n = read_varint(&b, &mut p)? as usize;
    let mut entries = vec![Entry { tile_id: 0, offset: 0, length: 0, run_length: 0 }; n];
    let mut last = 0u64;
    for e in entries.iter_mut() {
        last += read_varint(&b, &mut p)?;
        e.tile_id = last;
    }
    for e in entries.iter_mut() {
        e.run_length = read_varint(&b, &mut p)? as u32;
    }
    for e in entries.iter_mut() {
        e.length = read_varint(&b, &mut p)? as u32;
    }
    for i in 0..n {
        let v = read_varint(&b, &mut p)?;
        entries[i].offset = if v == 0 && i > 0 { entries[i - 1].offset + entries[i - 1].length as u64 } else { v - 1 };
    }
    Ok(entries)
}

/// Archive-wide values written into the header.
#[derive(Debug, Clone)]
pub struct Info {
    pub tile_type: TileType,
    /// Whether tile payloads are gzip-compressed (vector tiles).
    pub gzip_tiles: bool,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// `(west, south, east, north)` in degrees.
    pub bounds: (f64, f64, f64, f64),
    pub center: (f64, f64, u8),
}

/// Streams tiles into a scratch file; [`Writer::finish`] writes the archive in tile-id order.
pub struct Writer {
    path: PathBuf,
    tmp_path: PathBuf,
    tmp: BufWriter<File>,
    tmp_len: u64,
    /// `(tile id, scratch offset, length)`.
    tiles: Vec<(u64, u64, u32)>,
    small: HashMap<Vec<u8>, (u64, u32)>,
}

impl Writer {
    pub fn create(path: &Path) -> Result<Writer> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp_path = path.with_extension("pmtiles.data-tmp");
        let tmp = BufWriter::with_capacity(1 << 20, File::create(&tmp_path).with_context(|| format!("creating {}", tmp_path.display()))?);
        Ok(Writer { path: path.to_path_buf(), tmp_path, tmp, tmp_len: 0, tiles: Vec::new(), small: HashMap::new() })
    }

    pub fn add(&mut self, z: u8, x: u32, y: u32, data: &[u8]) -> Result<()> {
        let id = tile_id(z, x, y);
        if data.len() <= DEDUP_MAX
            && let Some(&(o, l)) = self.small.get(data)
        {
            self.tiles.push((id, o, l));
            return Ok(());
        }
        let o = self.tmp_len;
        self.tmp.write_all(data)?;
        self.tmp_len += data.len() as u64;
        if data.len() <= DEDUP_MAX {
            self.small.insert(data.to_vec(), (o, data.len() as u32));
        }
        self.tiles.push((id, o, data.len() as u32));
        Ok(())
    }

    /// Writes the archive (via a temporary name, then renamed) and removes the scratch file.
    pub fn finish(mut self, info: &Info, metadata: &serde_json::Value) -> Result<Stats> {
        self.tmp.flush()?;
        let scratch = File::open(&self.tmp_path)?;
        self.tiles.sort_by_key(|t| t.0);
        for w in self.tiles.windows(2) {
            ensure!(w[0].0 != w[1].0, "tile {:?} added twice", tile_zxy(w[0].0));
        }
        // Final data layout: payloads in tile-id order, each distinct payload once.
        let mut placed: HashMap<u64, u64> = HashMap::new();
        let mut order: Vec<(u64, u32)> = Vec::new();
        let mut data_len = 0u64;
        let mut entries: Vec<Entry> = Vec::new();
        for &(id, so, len) in &self.tiles {
            let fo = *placed.entry(so).or_insert_with(|| {
                let o = data_len;
                order.push((so, len));
                data_len += len as u64;
                o
            });
            if let Some(last) = entries.last_mut()
                && last.offset == fo
                && last.length == len
                && last.tile_id + last.run_length as u64 == id
            {
                last.run_length += 1;
                continue;
            }
            entries.push(Entry { tile_id: id, offset: fo, length: len, run_length: 1 });
        }
        let (root, leaves) = build_directories(&entries);
        let meta = gzip(serde_json::to_vec(metadata)?.as_slice());

        let final_tmp = self.path.with_extension("pmtiles.tmp");
        let mut out = BufWriter::with_capacity(1 << 20, File::create(&final_tmp)?);
        let root_off = HEADER_LEN as u64;
        let meta_off = root_off + root.len() as u64;
        let leaf_off = meta_off + meta.len() as u64;
        let data_off = leaf_off + leaves.len() as u64;
        let mut h = Vec::with_capacity(HEADER_LEN);
        h.extend_from_slice(b"PMTiles");
        h.push(3);
        for v in [root_off, root.len() as u64, meta_off, meta.len() as u64, leaf_off, leaves.len() as u64, data_off, data_len] {
            h.extend(v.to_le_bytes());
        }
        h.extend((self.tiles.len() as u64).to_le_bytes());
        h.extend((entries.len() as u64).to_le_bytes());
        h.extend((order.len() as u64).to_le_bytes());
        h.push(1); // clustered
        h.push(2); // internal compression: gzip
        h.push(if info.gzip_tiles { 2 } else { 1 });
        h.push(info.tile_type as u8);
        h.push(info.min_zoom);
        h.push(info.max_zoom);
        let e7 = |v: f64| ((v * 1e7).round() as i32).to_le_bytes();
        let (w, s, e, n) = info.bounds;
        for v in [w, s, e, n] {
            h.extend(e7(v));
        }
        h.push(info.center.2);
        h.extend(e7(info.center.0));
        h.extend(e7(info.center.1));
        debug_assert_eq!(h.len(), HEADER_LEN);
        out.write_all(&h)?;
        out.write_all(&root)?;
        out.write_all(&meta)?;
        out.write_all(&leaves)?;
        let mut buf = Vec::new();
        for &(so, len) in &order {
            buf.resize(len as usize, 0);
            scratch.read_exact_at(&mut buf, so)?;
            out.write_all(&buf)?;
        }
        out.flush()?;
        drop(out);
        std::fs::rename(&final_tmp, &self.path)?;
        std::fs::remove_file(&self.tmp_path).ok();
        Ok(Stats {
            addressed: self.tiles.len() as u64,
            contents: order.len() as u64,
            bytes: data_off + data_len,
        })
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        std::fs::remove_file(&self.tmp_path).ok();
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Stats {
    pub addressed: u64,
    pub contents: u64,
    pub bytes: u64,
}

/// Root directory and the concatenated leaf directories.
fn build_directories(entries: &[Entry]) -> (Vec<u8>, Vec<u8>) {
    let root = serialize_directory(entries);
    if root.len() <= ROOT_LIMIT {
        return (root, Vec::new());
    }
    let mut leaf_size = 4096usize;
    loop {
        let mut leaves = Vec::new();
        let mut root_entries = Vec::new();
        for chunk in entries.chunks(leaf_size) {
            let d = serialize_directory(chunk);
            root_entries.push(Entry { tile_id: chunk[0].tile_id, offset: leaves.len() as u64, length: d.len() as u32, run_length: 0 });
            leaves.extend(d);
        }
        let root = serialize_directory(&root_entries);
        if root.len() <= ROOT_LIMIT {
            return (root, leaves);
        }
        leaf_size *= 2;
    }
}

/// Reads an archive by byte ranges (validation and tests).
pub struct Reader {
    file: File,
    pub header: Header,
    root: Vec<Entry>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // the full header, as read; validation uses part of it
pub struct Header {
    pub root: (u64, u64),
    pub metadata: (u64, u64),
    pub leaves: (u64, u64),
    pub data: (u64, u64),
    pub addressed: u64,
    pub entries: u64,
    pub contents: u64,
    pub tile_compression: u8,
    pub tile_type: u8,
    pub min_zoom: u8,
    pub max_zoom: u8,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Reader> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut bytes = vec![0u8; HEADER_LEN];
        file.read_exact_at(&mut bytes, 0)?;
        ensure!(&bytes[0..7] == b"PMTiles" && bytes[7] == 3, "{} is not a PMTiles v3 archive", path.display());
        let u = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
        let header = Header {
            root: (u(8), u(16)),
            metadata: (u(24), u(32)),
            leaves: (u(40), u(48)),
            data: (u(56), u(64)),
            addressed: u(72),
            entries: u(80),
            contents: u(88),
            tile_compression: bytes[98],
            tile_type: bytes[99],
            min_zoom: bytes[100],
            max_zoom: bytes[101],
        };
        let mut r = Reader { file, header, root: Vec::new() };
        r.root = parse_directory(&r.range(r.header.root.0, r.header.root.1)?)?;
        Ok(r)
    }

    fn range(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut b = vec![0u8; len as usize];
        self.file.read_exact_at(&mut b, offset)?;
        Ok(b)
    }

    #[cfg(test)]
    pub fn metadata(&self) -> Result<serde_json::Value> {
        let (o, l) = self.header.metadata;
        Ok(serde_json::from_slice(&gunzip(&self.range(o, l)?)?)?)
    }

    fn find(dir: &[Entry], id: u64) -> Option<Entry> {
        let i = dir.partition_point(|e| e.tile_id <= id);
        if i == 0 {
            return None;
        }
        let e = dir[i - 1];
        if e.run_length == 0 || id < e.tile_id + e.run_length as u64 { Some(e) } else { None }
    }

    /// The stored tile payload (decompressed when the archive gzips tiles).
    pub fn get(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        let id = tile_id(z, x, y);
        let mut dir = self.root.clone();
        for _ in 0..4 {
            let Some(e) = Self::find(&dir, id) else { return Ok(None) };
            if e.run_length > 0 {
                let raw = self.range(self.header.data.0 + e.offset, e.length as u64)?;
                return Ok(Some(if self.header.tile_compression == 2 { gunzip(&raw)? } else { raw }));
            }
            dir = parse_directory(&self.range(self.header.leaves.0 + e.offset, e.length as u64)?)?;
        }
        bail!("directory nesting too deep")
    }

    /// All tile entries, by walking the directories.
    #[cfg(test)]
    pub fn all_entries(&self) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        for e in &self.root {
            if e.run_length > 0 {
                out.push(*e);
            } else {
                out.extend(parse_directory(&self.range(self.header.leaves.0 + e.offset, e.length as u64)?)?);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hilbert_ids() {
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 0, 0), 1);
        assert_eq!(tile_id(1, 0, 1), 2);
        assert_eq!(tile_id(1, 1, 1), 3);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 0, 0), 5);
        for z in 0..8u8 {
            let n = 1u32 << z;
            let mut seen = std::collections::HashSet::new();
            for x in 0..n {
                for y in 0..n {
                    let id = tile_id(z, x, y);
                    assert_eq!(tile_zxy(id), (z, x, y));
                    assert!(seen.insert(id));
                }
            }
        }
        assert_eq!(tile_zxy(tile_id(20, 123_456, 654_321)), (20, 123_456, 654_321));
    }

    #[test]
    fn directory_round_trip() {
        let entries: Vec<Entry> = (0..5000u64)
            .map(|i| Entry { tile_id: i * 3, offset: i * 10, length: 10, run_length: 1 + (i % 3) as u32 })
            .collect();
        assert_eq!(parse_directory(&serialize_directory(&entries)).unwrap(), entries);
    }

    #[test]
    fn archive_round_trip() {
        let dir = std::env::temp_dir().join(format!("pmtiles-test-{}", std::process::id()));
        let path = dir.join("t.pmtiles");
        let mut w = Writer::create(&path).unwrap();
        let mut expect = Vec::new();
        // Enough tiles to need leaf directories; many duplicates to exercise runs and dedup.
        for z in 0..=8u8 {
            let n = 1u32 << z;
            for x in 0..n {
                for y in 0..n {
                    if (x + y) % 7 == 3 {
                        continue;
                    }
                    let data: Vec<u8> = if (x ^ y) % 5 == 0 { vec![1, 2, 3] } else { format!("{z}/{x}/{y}").into_bytes() };
                    w.add(z, x, y, &data).unwrap();
                    expect.push((z, x, y, data));
                }
            }
        }
        let info = Info { tile_type: TileType::Png, gzip_tiles: false, min_zoom: 0, max_zoom: 8, bounds: (-180.0, -85.0, 180.0, 85.0), center: (0.0, 0.0, 0) };
        let stats = w.finish(&info, &serde_json::json!({"name": "test"})).unwrap();
        assert_eq!(stats.addressed as usize, expect.len());
        let r = Reader::open(&path).unwrap();
        assert!(r.header.leaves.1 > 0, "expected leaf directories");
        assert_eq!(r.metadata().unwrap()["name"], "test");
        for (z, x, y, data) in &expect {
            assert_eq!(r.get(*z, *x, *y).unwrap().as_deref(), Some(data.as_slice()), "{z}/{x}/{y}");
        }
        assert_eq!(r.get(3, 0, 3).unwrap(), None);
        let addressed: u64 = r.all_entries().unwrap().iter().map(|e| e.run_length as u64).sum();
        assert_eq!(addressed, expect.len() as u64);
        std::fs::remove_dir_all(&dir).ok();
    }
}
