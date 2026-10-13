//! Reader for `TIMELINE.DAT`, the Encarta Timeline database.
//!
//! Spec: `docs/formats/timeline.md`. The file is a small descriptor, the category and region flag
//! lists, two tables of 70-byte records (dated events and undated search items), a UTF-16 title
//! pool, and a term index (term → event refIds) whose term texts live in `TIMELINE.EIT`.

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

/// Size of one event or item record.
pub const RECORD_LEN: usize = 70;
/// Size of one term-table entry.
pub const TERM_LEN: usize = 16;

const DESCRIPTOR_AT: usize = 0x18;
const DESCRIPTOR_LEN: u32 = 28;

/// Date-flag bits (record offset 0).
pub mod flags {
    /// Drawn as a period rather than a point (inferred).
    pub const SPAN: u32 = 0x01;
    /// Start date is approximate (inferred).
    pub const START_CIRCA: u32 = 0x02;
    pub const START_BC: u32 = 0x04;
    /// End date is approximate (inferred).
    pub const END_CIRCA: u32 = 0x08;
    pub const END_BC: u32 = 0x10;
    /// Start year magnitude counts thousands of years.
    pub const START_THOUSANDS: u32 = 0x20;
    /// End year magnitude counts thousands of years.
    pub const END_THOUSANDS: u32 = 0x40;
}

#[derive(Debug, Clone, Serialize)]
pub struct Timeline {
    pub sections: Sections,
    pub categories: Vec<NamedBit>,
    pub regions: Vec<NamedBit>,
    /// Dated timeline events, sorted by start date.
    pub events: Vec<Record>,
    /// Undated search targets (topic timelines and search-result ids).
    pub items: Vec<Record>,
    /// Term index; term `k` is item `k` of `TIMELINE.EIT/$WW$CTCATALOG`.
    pub terms: Vec<Term>,
}

/// Byte lengths from the section descriptor at 0x18.
#[derive(Debug, Clone, Serialize)]
pub struct Sections {
    pub event_bytes: u32,
    pub item_bytes: u32,
    pub string_bytes: u32,
    /// Always 1 on the disc; meaning unknown.
    pub unknown: u32,
    pub term_bytes: u32,
    pub term_ref_bytes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedBit {
    pub name: String,
    pub bit: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub flags: u32,
    pub title: String,
    pub title_offset: u32,
    pub title_len: u32,
    pub start: Option<Date>,
    pub end: Option<Date>,
    pub span: bool,
    pub importance: u16,
    pub category_mask: u16,
    pub region_mask: u16,
    pub seq: u32,
    pub ref_id: u32,
    /// 1 = event, 3 = essay/feature, 0 = item.
    pub kind: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Date {
    /// Signed year: negative is BC (-1 = 1 BC; there is no year 0).
    pub year: i64,
    /// Raw month field: 1–12, 0 = unknown (out-of-range values are data errors).
    pub month: u16,
    /// Raw day field: 1–31, 0 = unknown.
    pub day: u16,
    pub circa: bool,
    /// The stored magnitude was in thousands of years.
    pub thousands: bool,
    /// The magnitude as stored.
    pub raw_year: u32,
}

impl Date {
    fn decode(raw_year: u32, month: u16, day: u16, bc: bool, thousands: bool, circa: bool) -> Self {
        let mut year = i64::from(raw_year);
        if thousands {
            year *= 1000;
        }
        if bc {
            year = -year;
        }
        Date { year, month, day, circa, thousands, raw_year }
    }

    /// Month and day are within their calendar ranges (0 allowed for "unknown").
    pub fn is_well_formed(&self) -> bool {
        self.month <= 12 && self.day <= 31
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Term {
    /// Unresolved cumulative offset into an unknown string pool.
    pub aux_offset: u32,
    /// Cumulative byte offset of this term's UTF-16Z text (text comes from TIMELINE.EIT).
    pub text_offset: u32,
    /// Event refIds that this term finds.
    pub ref_ids: Vec<u32>,
}

impl Timeline {
    /// Category names whose bit is set in `mask`.
    pub fn category_names(&self, mask: u16) -> Vec<&str> {
        names(&self.categories, mask)
    }

    /// Region names whose bit is set in `mask`.
    pub fn region_names(&self, mask: u16) -> Vec<&str> {
        names(&self.regions, mask)
    }
}

fn names(list: &[NamedBit], mask: u16) -> Vec<&str> {
    list.iter()
        .filter(|n| u32::from(mask) & n.bit != 0)
        .map(|n| n.name.as_str())
        .collect()
}

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len());
        let Some(end) = end else {
            bail!("truncated at {:#x} (need {} bytes)", self.pos, n)
        };
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(le32(self.take(4)?, 0))
    }

    fn utf16z(&mut self) -> Result<String> {
        let start = self.pos;
        let mut units = Vec::new();
        loop {
            let u = le16(self.take(2).with_context(|| format!("string at {start:#x}"))?, 0);
            if u == 0 {
                break;
            }
            units.push(u);
        }
        String::from_utf16(&units).with_context(|| format!("bad UTF-16 at {start:#x}"))
    }
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

pub fn parse(bytes: &[u8]) -> Result<Timeline> {
    let mut c = Cursor { b: bytes, pos: DESCRIPTOR_AT };
    let descriptor_len = c.u32()?;
    ensure!(descriptor_len == DESCRIPTOR_LEN, "descriptor length {descriptor_len}, expected 28");
    let sections = Sections {
        event_bytes: c.u32()?,
        item_bytes: c.u32()?,
        string_bytes: c.u32()?,
        unknown: c.u32()?,
        term_bytes: c.u32()?,
        term_ref_bytes: c.u32()?,
    };
    let n_categories = c.u32()? as usize;
    let n_regions = c.u32()? as usize;
    ensure!(n_categories <= 32 && n_regions <= 32, "implausible list counts");
    let mut read_list = |n: usize| -> Result<Vec<NamedBit>> {
        (0..n)
            .map(|_| Ok(NamedBit { name: c.utf16z()?, bit: c.u32()? }))
            .collect()
    };
    let categories = read_list(n_categories)?;
    let regions = read_list(n_regions)?;

    let s = &sections;
    ensure!((s.event_bytes as usize).is_multiple_of(RECORD_LEN), "event table not a multiple of 70");
    ensure!((s.item_bytes as usize).is_multiple_of(RECORD_LEN), "item table not a multiple of 70");
    ensure!((s.term_bytes as usize).is_multiple_of(TERM_LEN), "term table not a multiple of 16");
    ensure!(s.term_ref_bytes.is_multiple_of(4), "reference pool not a multiple of 4");
    let event_raw = c.take(s.event_bytes as usize).context("event table")?;
    let item_raw = c.take(s.item_bytes as usize).context("item table")?;
    let pool = c.take(s.string_bytes as usize).context("title pool")?;
    let term_raw = c.take(s.term_bytes as usize).context("term table")?;
    let ref_raw = c.take(s.term_ref_bytes as usize).context("reference pool")?;
    ensure!(c.pos == bytes.len(), "{} trailing bytes", bytes.len() - c.pos);

    let records = |raw: &[u8]| -> Result<Vec<Record>> {
        raw.chunks_exact(RECORD_LEN)
            .enumerate()
            .map(|(i, r)| parse_record(r, pool).with_context(|| format!("record {i}")))
            .collect()
    };
    let events = records(event_raw)?;
    let items = records(item_raw)?;

    let refs: Vec<u32> = ref_raw.chunks_exact(4).map(|b| le32(b, 0)).collect();
    let mut terms = Vec::with_capacity(term_raw.len() / TERM_LEN);
    for (k, e) in term_raw.chunks_exact(TERM_LEN).enumerate() {
        let (ref_off, count) = (le32(e, 8) as usize, le32(e, 12) as usize);
        ensure!(ref_off.is_multiple_of(4), "term {k}: unaligned reference offset");
        let first = ref_off / 4;
        let ids = first
            .checked_add(count)
            .and_then(|end| refs.get(first..end))
            .with_context(|| format!("term {k}: references out of range"))?;
        terms.push(Term { aux_offset: le32(e, 0), text_offset: le32(e, 4), ref_ids: ids.to_vec() });
    }

    Ok(Timeline { sections, categories, regions, events, items, terms })
}

fn parse_record(r: &[u8], pool: &[u8]) -> Result<Record> {
    let flags = le32(r, 0x00);
    let (title_offset, title_len) = (le32(r, 0x04), le32(r, 0x08));
    let (off, len) = (title_offset as usize, title_len as usize);
    ensure!(len >= 2 && len.is_multiple_of(2), "bad title length {len}");
    let raw = off
        .checked_add(len)
        .and_then(|end| pool.get(off..end))
        .context("title outside pool")?;
    ensure!(raw[len - 2..] == [0, 0], "title not NUL-terminated");
    let units: Vec<u16> = raw[..len - 2].chunks_exact(2).map(|u| le16(u, 0)).collect();
    let title = String::from_utf16(&units).context("bad UTF-16 title")?;

    let has = |bit: u32| flags & bit != 0;
    let start_year = le32(r, 0x24);
    let end_year = le32(r, 0x2C);
    let start = (start_year != 0).then(|| {
        Date::decode(
            start_year,
            le16(r, 0x28),
            le16(r, 0x2A),
            has(flags::START_BC),
            has(flags::START_THOUSANDS),
            has(flags::START_CIRCA),
        )
    });
    let end = (end_year != 0).then(|| {
        Date::decode(
            end_year,
            le16(r, 0x30),
            le16(r, 0x32),
            has(flags::END_BC),
            has(flags::END_THOUSANDS),
            has(flags::END_CIRCA),
        )
    });
    Ok(Record {
        flags,
        title,
        title_offset,
        title_len,
        start,
        end,
        span: has(flags::SPAN),
        importance: le16(r, 0x34),
        category_mask: le16(r, 0x36),
        region_mask: le16(r, 0x38),
        seq: le32(r, 0x3A),
        ref_id: le32(r, 0x3E),
        kind: le16(r, 0x42),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16z(s: &str) -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
    }

    struct Rec {
        flags: u32,
        start: (u32, u16, u16),
        end: (u32, u16, u16),
        seq: u32,
        ref_id: u32,
        kind: u16,
    }

    fn record(r: &Rec, title_off: u32, title_len: u32) -> Vec<u8> {
        let mut v = vec![0u8; RECORD_LEN];
        v[0..4].copy_from_slice(&r.flags.to_le_bytes());
        v[4..8].copy_from_slice(&title_off.to_le_bytes());
        v[8..12].copy_from_slice(&title_len.to_le_bytes());
        v[0x24..0x28].copy_from_slice(&r.start.0.to_le_bytes());
        v[0x28..0x2A].copy_from_slice(&r.start.1.to_le_bytes());
        v[0x2A..0x2C].copy_from_slice(&r.start.2.to_le_bytes());
        v[0x2C..0x30].copy_from_slice(&r.end.0.to_le_bytes());
        v[0x30..0x32].copy_from_slice(&r.end.1.to_le_bytes());
        v[0x32..0x34].copy_from_slice(&r.end.2.to_le_bytes());
        v[0x34..0x36].copy_from_slice(&2u16.to_le_bytes());
        v[0x36..0x38].copy_from_slice(&1u16.to_le_bytes());
        v[0x38..0x3A].copy_from_slice(&3u16.to_le_bytes());
        v[0x3A..0x3E].copy_from_slice(&r.seq.to_le_bytes());
        v[0x3E..0x42].copy_from_slice(&r.ref_id.to_le_bytes());
        v[0x42..0x44].copy_from_slice(&r.kind.to_le_bytes());
        v
    }

    /// A synthetic file: 2 categories, 2 regions, 2 events, 1 item, 2 terms.
    fn sample() -> Vec<u8> {
        let titles = ["Rocks Cool", "Wall Built", ""];
        let mut pool = Vec::new();
        let mut spans = Vec::new();
        for t in titles {
            let s = utf16z(t);
            spans.push((pool.len() as u32, s.len() as u32));
            pool.extend(s);
        }
        let recs = [
            Rec { flags: 0x25, start: (3_000, 0, 0), end: (12, 0, 0), seq: 1, ref_id: 900, kind: 1 },
            Rec { flags: 0x02, start: (1961, 8, 13), end: (0, 0, 0), seq: 2, ref_id: 901, kind: 3 },
            Rec { flags: 0, start: (0, 0, 0), end: (0, 0, 0), seq: 3, ref_id: 902, kind: 0 },
        ];
        let refs: Vec<u32> = vec![900, 901, 900];
        let terms: [(u32, u32, u32, u32); 2] = [(0, 0, 0, 2), (10, 12, 8, 1)];

        let mut lists = Vec::new();
        for (name, bit) in [("Arts", 2u32), ("Science", 1), ("Europe", 1), ("Global", 0x10)] {
            lists.extend(utf16z(name));
            lists.extend(bit.to_le_bytes());
        }
        let mut f = vec![0u8; DESCRIPTOR_AT];
        for v in [
            DESCRIPTOR_LEN,
            2 * RECORD_LEN as u32,
            RECORD_LEN as u32,
            pool.len() as u32,
            1,
            (terms.len() * TERM_LEN) as u32,
            (refs.len() * 4) as u32,
            2,
            2,
        ] {
            f.extend(v.to_le_bytes());
        }
        f.extend(lists);
        for (r, &(o, l)) in recs.iter().zip(&spans) {
            f.extend(record(r, o, l));
        }
        f.extend(&pool);
        for (a, b, c, d) in terms {
            for v in [a, b, c, d] {
                f.extend(v.to_le_bytes());
            }
        }
        for r in refs {
            f.extend(r.to_le_bytes());
        }
        f
    }

    #[test]
    fn parses_synthetic_file() {
        let t = parse(&sample()).unwrap();
        assert_eq!(t.categories.len(), 2);
        assert_eq!(t.regions[1].name, "Global");
        assert_eq!(t.events.len(), 2);
        assert_eq!(t.items.len(), 1);

        let e = &t.events[0];
        assert_eq!(e.title, "Rocks Cool");
        assert!(e.span);
        let s = e.start.unwrap();
        assert_eq!((s.year, s.thousands, s.circa), (-3_000_000, true, false));
        let end = e.end.unwrap();
        assert_eq!((end.year, end.thousands), (12, false));
        assert_eq!(t.category_names(e.category_mask), ["Science"]);
        assert_eq!(t.region_names(e.region_mask), ["Europe"]);

        let w = &t.events[1];
        let s = w.start.unwrap();
        assert_eq!((s.year, s.month, s.day, s.circa), (1961, 8, 13, true));
        assert!(w.end.is_none());
        assert_eq!(w.kind, 3);

        let i = &t.items[0];
        assert_eq!((i.title.as_str(), i.start, i.ref_id), ("", None, 902));

        assert_eq!(t.terms[0].ref_ids, [900, 901]);
        assert_eq!(t.terms[1].ref_ids, [900]);
        assert_eq!(t.terms[1].text_offset, 12);
    }

    #[test]
    fn rejects_truncation_and_bad_references() {
        let f = sample();
        assert!(parse(&f[..f.len() - 1]).is_err());

        let mut g = f.clone();
        g.push(0);
        assert!(parse(&g).is_err(), "trailing bytes");

        // Point the second term past the end of the reference pool.
        let mut h = f.clone();
        let term1 = h.len() - 3 * 4 - TERM_LEN;
        h[term1 + 12..term1 + 16].copy_from_slice(&5u32.to_le_bytes());
        assert!(parse(&h).is_err());
    }

    #[test]
    fn decodes_date_flags() {
        let d = Date::decode(65_000, 0, 0, true, true, false);
        assert_eq!(d.year, -65_000_000);
        let d = Date::decode(1492, 10, 12, false, false, false);
        assert!(d.is_well_formed());
        let d = Date::decode(1492, 12, 1492, false, false, false);
        assert!(!d.is_well_formed());
    }
}
