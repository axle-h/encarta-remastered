//! The record index at the end of an AKC file (spec §3).

use super::bits::BitReader;
use anyhow::{Context, Result, bail, ensure};

/// One field descriptor of the bit-area header: values are `base + read(width)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Field {
    pub base: u64,
    pub width: u32,
    pub max: u64,
}

impl Field {
    fn read_desc(r: &mut BitReader) -> Field {
        let base = r.read(32) as u64;
        let width = r.read(5);
        let range = r.read(width) as u64;
        Field {
            base,
            width,
            max: base + range,
        }
    }
    #[inline]
    fn value(&self, r: &mut BitReader) -> u64 {
        self.base + r.read(self.width) as u64
    }
}

/// The twelve descriptors plus the key-delta width.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexHeader {
    pub key_width: u32,
    /// Direct records: count, ulen, clen, offset, ntok, unused.
    pub direct: [Field; 6],
    /// Block entries: count, ulen base, clen, offset, ntok base, unused.
    pub shared: [Field; 6],
}

/// Where a record's bytes come from, as stored in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRecord {
    pub key: u32,
    pub direct: bool,
    pub offset: u64,
    pub clen: u64,
    pub ulen: u64,
    pub ntok: u64,
    pub start: u64,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub header: IndexHeader,
    /// Sparse table: `(first_key, bit_offset)` per group.
    pub groups: Vec<(u32, u32)>,
    /// Number of block entries `M`.
    pub block_entries: usize,
    pub records: Vec<IndexRecord>,
}

#[derive(Debug, Clone, Copy)]
struct BlockEntry {
    offset: u64,
    clen: u64,
    ulen_base: u64,
    ulen_w: u32,
    ntok_base: u64,
    ntok_w: u32,
    start_w: u32,
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .with_context(|| format!("index truncated at byte {o}"))
}

/// Number of records in each sparse group: groups of 32 for records `0 … N−2`,
/// then the last record alone.
fn group_sizes(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let g = (n - 1).div_ceil(32);
    let mut v: Vec<usize> = (0..g).map(|i| (n - 1 - 32 * i).min(32)).collect();
    v.push(1);
    v
}

impl Index {
    /// Parses the index stream (`data` = the bytes at header offset 0x24).
    pub fn parse(data: &[u8]) -> Result<Index> {
        ensure!(
            data.len().is_multiple_of(2),
            "index has odd length {}",
            data.len()
        );
        let n = u32_at(data, 0)? as usize;
        let sizes = group_sizes(n);
        let g1 = sizes.len().max(1); // G + 1 sparse entries (N = 0 still has one slot)
        let mut groups = Vec::with_capacity(g1);
        for i in 0..g1 {
            groups.push((u32_at(data, 4 + 8 * i)?, u32_at(data, 8 + 8 * i)?));
        }
        let m_at = 4 + 8 * g1;
        let m = u32_at(data, m_at)? as usize;
        let mut entry_bits = Vec::with_capacity(m);
        for i in 0..m {
            entry_bits.push(u32_at(data, m_at + 4 + 4 * i)? as u64);
        }
        let area = (m_at + 4 + 4 * m) as u64 * 8;

        let mut r = BitReader::at(data, area)?;
        let mut h = IndexHeader {
            key_width: r.read(5),
            ..Default::default()
        };
        for f in h.direct.iter_mut().chain(h.shared.iter_mut()) {
            *f = Field::read_desc(&mut r);
        }
        r.check("index header")?;
        let [s_count, s_ulen, s_clen, s_off, s_ntok, _] = h.shared;
        let [d_count, d_ulen, d_clen, d_off, d_ntok, _] = h.direct;
        ensure!(
            s_count.max == m as u64,
            "index header says {} block entries, table has {m}",
            s_count.max
        );

        let mut entries = Vec::with_capacity(m);
        for (e, &bit) in entry_bits.iter().enumerate() {
            let mut er = BitReader::at(data, bit).with_context(|| format!("block entry {e}"))?;
            let be = BlockEntry {
                offset: s_off.value(&mut er),
                clen: s_clen.value(&mut er),
                ulen_base: s_ulen.value(&mut er),
                ulen_w: er.read(5),
                ntok_base: s_ntok.value(&mut er),
                ntok_w: er.read(5),
                start_w: er.read(5),
            };
            er.check("block entry")?;
            entries.push(be);
        }

        let mut records = Vec::with_capacity(n);
        for (gi, &count) in sizes.iter().enumerate() {
            let (first_key, bit) = groups[gi];
            r.seek(bit as u64)
                .with_context(|| format!("sparse entry {gi}"))?;
            let mut key = first_key as u64;
            for j in 0..count {
                if j > 0 {
                    let delta = if h.key_width != 0 && r.bit() {
                        r.read(h.key_width) as u64
                    } else {
                        1
                    };
                    key += delta;
                }
                ensure!(key <= u32::MAX as u64, "record key overflow");
                let direct = if d_count.max == 0 {
                    false
                } else if s_count.max == 0 {
                    true
                } else {
                    r.bit()
                };
                let rec = if direct {
                    IndexRecord {
                        key: key as u32,
                        direct,
                        offset: d_off.value(&mut r),
                        clen: d_clen.value(&mut r),
                        ulen: d_ulen.value(&mut r),
                        ntok: d_ntok.value(&mut r),
                        start: 0,
                    }
                } else {
                    let e = r.read(s_count.width) as usize;
                    let Some(be) = entries.get(e) else {
                        bail!("record {key}: block entry {e} >= {m}");
                    };
                    IndexRecord {
                        key: key as u32,
                        direct,
                        offset: be.offset,
                        clen: be.clen,
                        ulen: be.ulen_base + r.read(be.ulen_w) as u64,
                        ntok: be.ntok_base + r.read(be.ntok_w) as u64,
                        start: r.read(be.start_w) as u64,
                    }
                };
                records.push(rec);
            }
            r.check("record group")?;
        }
        Ok(Index {
            header: h,
            groups,
            block_entries: m,
            records,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::bits::tests::BitWriter;
    use super::*;

    #[test]
    fn grouping() {
        assert_eq!(group_sizes(1), vec![1]);
        assert_eq!(group_sizes(2), vec![1, 1]);
        assert_eq!(group_sizes(33), vec![32, 1]);
        assert_eq!(group_sizes(34), vec![32, 1, 1]);
        let s = group_sizes(44_055);
        assert_eq!(s.len(), 1_378);
        assert_eq!((s[1_375], s[1_376], s[1_377]), (32, 22, 1));
    }

    /// One field descriptor with `base` and a value width `w` and range.
    fn desc(w: &mut BitWriter, base: u64, width: u32, range: u64) {
        w.put(32, base).put(5, width as u64).put(width, range);
    }

    /// Builds an index with records: key 0 direct, keys 1 and 2 in block entry 0, key 4 direct.
    pub fn synthetic_index() -> Vec<u8> {
        // N = 4 → G = 1: group 0 holds records 0..3 (3 records), entry 1 holds the last.
        let n = 4u32;
        let m = 1u32;
        let fixed_bytes = 4 + 8 * 2 + 4 + 4 * m as usize;
        let mut w = BitWriter::default();
        w.put(5, 2); // kw
        // Direct: count 2, ulen base 100 w 8, clen base 10 w 8, offset 1000 w 16, ntok base 5 w 8, unused.
        desc(&mut w, 2, 2, 0);
        desc(&mut w, 100, 8, 255);
        desc(&mut w, 10, 8, 255);
        desc(&mut w, 1000, 16, 65_535);
        desc(&mut w, 5, 8, 255);
        desc(&mut w, 0, 0, 0);
        // Shared: count 1 (w 1), ulen base 0 w 8, clen 20 w 8, offset 2000 w 16, ntok base 0 w 8.
        desc(&mut w, 0, 1, 1);
        desc(&mut w, 0, 8, 255);
        desc(&mut w, 20, 8, 255);
        desc(&mut w, 2000, 16, 65_535);
        desc(&mut w, 0, 8, 255);
        desc(&mut w, 0, 0, 0);
        let entry0 = fixed_bytes as u64 * 8 + w.len() as u64;
        // Block entry 0: offset 2000+4, clen 20+30, ulen base 40, ulen_w 4, ntok base 3, ntok_w 2, start_w 4.
        w.put(16, 4)
            .put(8, 30)
            .put(8, 40)
            .put(5, 4)
            .put(8, 3)
            .put(5, 2)
            .put(5, 4);
        let group0 = fixed_bytes as u64 * 8 + w.len() as u64;
        // key 0: direct; offset 1000+0, clen 10+2, ulen 100+7, ntok 5+1.
        w.put(1, 1).put(16, 0).put(8, 2).put(8, 7).put(8, 1);
        // key 1 (delta flag 0): member e=0: ulen 40+1, ntok 3+0, start 0.
        w.put(1, 0)
            .put(1, 0)
            .put(1, 0)
            .put(4, 1)
            .put(2, 0)
            .put(4, 0);
        // key 2: member: ulen 40+2, ntok 3+1, start 3.
        w.put(1, 0)
            .put(1, 0)
            .put(1, 0)
            .put(4, 2)
            .put(2, 1)
            .put(4, 3);
        let group1 = fixed_bytes as u64 * 8 + w.len() as u64;
        // last record, key 4 (sparse entry gives it directly): direct.
        w.put(1, 1).put(16, 500).put(8, 4).put(8, 9).put(8, 0);
        while w.len() % 16 != 0 {
            w.put(1, 0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&n.to_le_bytes());
        for (k, b) in [(0u32, group0 as u32), (4, group1 as u32)] {
            out.extend_from_slice(&k.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        out.extend_from_slice(&m.to_le_bytes());
        out.extend_from_slice(&(entry0 as u32).to_le_bytes());
        assert_eq!(out.len(), fixed_bytes);
        out.extend_from_slice(&w.bytes());
        out
    }

    #[test]
    fn synthetic() {
        let idx = Index::parse(&synthetic_index()).unwrap();
        assert_eq!(idx.header.key_width, 2);
        let r = &idx.records;
        assert_eq!(r.len(), 4);
        assert_eq!(
            r[0],
            IndexRecord {
                key: 0,
                direct: true,
                offset: 1000,
                clen: 12,
                ulen: 107,
                ntok: 6,
                start: 0
            }
        );
        assert_eq!((r[1].key, r[1].offset, r[1].clen), (1, 2004, 50));
        assert_eq!((r[1].ulen, r[1].ntok, r[1].start), (41, 3, 0));
        assert_eq!((r[2].key, r[2].ulen, r[2].ntok, r[2].start), (2, 42, 4, 3));
        assert_eq!((r[3].key, r[3].offset, r[3].ulen), (4, 1500, 109));
    }
}
