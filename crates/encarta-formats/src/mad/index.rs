//! `INDX` key → (offset, length) index (mad.md §2).

use anyhow::{Result, bail, ensure};

const PAGE: usize = 2048;

/// Consecutive keys `first … first+count−1` that all map to one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRun {
    pub first: u32,
    pub count: u32,
    pub offset: u32,
    pub length: u32,
}

impl KeyRun {
    pub fn last(&self) -> u32 {
        self.first + (self.count - 1)
    }
}

/// A fully decoded index. Keys with a zero length are absent.
#[derive(Debug, Clone)]
pub struct Index {
    pub last_keys: Vec<u32>,
    runs: Vec<KeyRun>,
}

impl Index {
    pub fn parse(b: &[u8]) -> Result<Index> {
        ensure!(b.len() >= 8 && &b[0..4] == b"INDX", "not an INDX section");
        let npages = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
        let pages_at = 8 + 4 * npages;
        ensure!(b.len() >= pages_at + PAGE * npages, "INDX with {npages} pages is truncated");
        let last_keys: Vec<u32> =
            (0..npages).map(|p| u32::from_le_bytes(b[8 + 4 * p..12 + 4 * p].try_into().unwrap())).collect();
        let mut runs = Vec::new();
        for p in 0..npages {
            let first = if p == 0 { 0 } else { last_keys[p - 1] as u64 + 1 };
            let page = &b[pages_at + PAGE * p..pages_at + PAGE * (p + 1)];
            decode_page(page, first, last_keys[p] as u64, &mut runs).map_err(|e| e.context(format!("INDX page {p}")))?;
        }
        Ok(Index { last_keys, runs })
    }

    /// Every key run in key order.
    pub fn runs(&self) -> &[KeyRun] {
        &self.runs
    }

    /// Number of keys that have a record.
    pub fn key_count(&self) -> u64 {
        self.runs.iter().map(|r| r.count as u64).sum()
    }

    /// Distinct records `(offset, length)` in offset order, each with the first key that uses it.
    pub fn records(&self) -> Vec<(u32, u32, u32)> {
        let mut v: Vec<(u32, u32, u32)> = self.runs.iter().map(|r| (r.offset, r.length, r.first)).collect();
        v.sort_unstable();
        v.dedup_by_key(|r| (r.0, r.1));
        v
    }

    /// The run containing `key`.
    pub fn run(&self, key: u32) -> Option<&KeyRun> {
        let i = self.runs.partition_point(|r| r.last() < key);
        self.runs.get(i).filter(|r| r.first <= key)
    }

    /// `(offset, length)` of the record for `key`.
    pub fn lookup(&self, key: u32) -> Option<(u32, u32)> {
        self.run(key).map(|r| (r.offset, r.length))
    }

    /// Byte offset just past the last record (the data area must be at least this long).
    pub fn data_end(&self) -> u64 {
        self.runs.iter().map(|r| r.offset as u64 + r.length as u64).max().unwrap_or(0)
    }
}

fn decode_page(page: &[u8], first: u64, last: u64, runs: &mut Vec<KeyRun>) -> Result<()> {
    let mut key = first;
    let mut off = u32::from_le_bytes(page[0..4].try_into().unwrap()) as u64;
    let mut len: u64 = 0;
    let mut pos = 4usize;
    let byte = |pos: &mut usize| -> Result<u64> {
        let Some(&c) = page.get(*pos) else { bail!("op stream runs off the page") };
        *pos += 1;
        Ok(c as u64)
    };
    // Appends `n` keys starting at `key` that map to the current (off, len).
    let mut emit = |key: u64, n: u64, off: u64, len: u64| -> Result<()> {
        if n == 0 || len == 0 {
            return Ok(());
        }
        ensure!(key + n - 1 <= last, "keys {key}+{n} exceed the page's last key {last}");
        ensure!(off + len <= u32::MAX as u64, "record offset overflow");
        if let Some(prev) = runs.last_mut()
            && prev.offset as u64 == off
            && prev.length as u64 == len
            && prev.last() as u64 + 1 == key
        {
            prev.count += n as u32;
            return Ok(());
        }
        runs.push(KeyRun { first: key as u32, count: n as u32, offset: off as u32, length: len as u32 });
        Ok(())
    };
    while key <= last {
        let c = byte(&mut pos)?;
        if c < 0x80 || c & 0xFC == 0xFC || c & 0xF8 == 0xF0 {
            let new_len = if c < 0x80 {
                (c << 8) | byte(&mut pos)?
            } else if c & 0xFC == 0xFC {
                ((c & 3) << 16) | byte(&mut pos)? << 8 | byte(&mut pos)?
            } else {
                (byte(&mut pos)? & 0x7F) << 24 | byte(&mut pos)? << 16 | byte(&mut pos)? << 8 | byte(&mut pos)?
            };
            off += len;
            len = new_len;
            emit(key, 1, off, len)?;
            key += 1;
            continue;
        }
        let (skip, n) = match c & 0xE0 {
            0x80 => (true, c & 0x1F),
            0xA0 if c & 0x10 == 0 => (true, (c & 0x0F) << 8 | byte(&mut pos)?),
            0xA0 => (true, (c & 0x0F) << 24 | byte(&mut pos)? << 16 | byte(&mut pos)? << 8 | byte(&mut pos)?),
            0xC0 => (false, c & 0x1F),
            _ if c & 0x08 == 0 => (false, (c & 7) << 8 | byte(&mut pos)?),
            _ => (false, ((c & 7) << 8 | byte(&mut pos)?) << 8 | byte(&mut pos)?),
        };
        if !skip {
            emit(key, n, off, len)?;
        }
        key += n;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Builds an INDX section from page op streams: `(last_key, base, ops)`.
    pub fn build(pages: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        let mut b = b"INDX".to_vec();
        b.extend((pages.len() as u32).to_le_bytes());
        for (last, _, _) in pages {
            b.extend(last.to_le_bytes());
        }
        for (_, base, ops) in pages {
            let mut page = base.to_le_bytes().to_vec();
            page.extend(ops);
            page.resize(super::PAGE, 0xEE);
            b.extend(page);
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_all_op_classes() {
        let ops = vec![
            0x83, // skip keys 0-2
            0x00, 0x10, // key 3 → (100, 16)
            0xC2, // keys 4, 5 repeat
            0xFD, 0x00, 0x01, // key 6 → (116, 0x10001)
            0xA0, 0x02, // skip 2: keys 7, 8
            0xE0, 0x01, // repeat 1: key 9 → (116, 0x10001), not contiguous with key 6
            0xF0, 0x80, 0x00, 0x00, 0x05, // key 10 → (116+0x10001, 5)
            0xB0, 0x00, 0x00, 0x05, // skip 5: 11-15
            0xE8, 0x00, 0x02, // repeat 2: 16, 17
            0x00, 0x00, // key 18: zero length → absent
            0xC1, // key 19 repeats the zero length → absent
            0x00, 0x03, // key 20 → (116+0x10001+5, 3)
        ];
        let b = testutil::build(&[(20, 100, ops), (25, 999, vec![0x85, 0x00, 0x07])]);
        let idx = Index::parse(&b).unwrap();
        assert_eq!(idx.lookup(2), None);
        assert_eq!(idx.lookup(3), Some((100, 16)));
        assert_eq!(idx.lookup(5), Some((100, 16)));
        assert_eq!(idx.lookup(6), Some((116, 0x10001)));
        assert_eq!(idx.lookup(8), None);
        assert_eq!(idx.lookup(9), Some((116, 0x10001)));
        let o10 = 116 + 0x10001;
        assert_eq!(idx.lookup(10), Some((o10, 5)));
        assert_eq!(idx.lookup(17), Some((o10, 5)));
        assert_eq!(idx.lookup(18), None);
        assert_eq!(idx.lookup(19), None);
        assert_eq!(idx.lookup(20), Some((o10 + 5, 3)));
        // Page 2: first key 21, skips 21-25: nothing; the trailing entry is padding.
        assert_eq!(idx.lookup(21), None);
        assert_eq!(idx.lookup(26), None);
        assert_eq!(idx.key_count(), 3 + 2 + 3 + 1);
        assert_eq!(idx.records().len(), 4);
    }

    #[test]
    fn rejects_bad_magic_and_overrun() {
        assert!(Index::parse(b"XXXX\0\0\0\0").is_err());
        let b = testutil::build(&[(1, 0, vec![0xC5])]); // repeat with no previous entry: ignored
        assert_eq!(Index::parse(&b).unwrap().key_count(), 0);
        let b = testutil::build(&[(1, 0, vec![0x00, 0x01, 0xC5])]); // repeat past the last key
        assert!(Index::parse(&b).is_err());
    }
}
