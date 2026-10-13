//! Decodes the disc's TIMELINE.DAT and checks structural invariants and cross-references.
//! Reads `$ENCARTA_EXTRACTED` (default `<workspace>/extracted`); skipped when the data is absent.

use std::collections::HashSet;
use std::path::PathBuf;

use encarta_formats::timeline;

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

/// The refId column of a catalog2 ECN file (`"ECN\0" | N locations | N refIds | u32 N`).
fn ecn_ids(path: &std::path::Path) -> HashSet<u32> {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[..4], b"ECN\0");
    let n = u32::from_le_bytes(b[b.len() - 4..].try_into().unwrap()) as usize;
    let ids = &b[4 + 4 * n..4 + 8 * n];
    ids.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

#[test]
fn disc_timeline() {
    let root = extracted();
    let path = root.join("iso/EE/ENCARTA/TIMELINE.DAT");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: {} not found", path.display());
        return;
    };
    let t = timeline::parse(&bytes).unwrap();

    assert_eq!(t.categories.len(), 6);
    assert_eq!(t.regions.len(), 9);
    assert_eq!(t.events.len(), 3404);
    assert_eq!(t.items.len(), 13061);
    assert_eq!(t.terms.len(), 22749);
    assert_eq!(t.sections.unknown, 1);

    // Flags are distinct single bits and the record masks use only them.
    for list in [&t.categories, &t.regions] {
        let all = list.iter().fold(0u32, |acc, n| {
            assert_eq!(n.bit.count_ones(), 1, "{}", n.name);
            assert_eq!(acc & n.bit, 0, "duplicate bit {}", n.name);
            acc | n.bit
        });
        assert!(all.count_ones() as usize == list.len());
    }
    let cat_all: u32 = t.categories.iter().map(|n| n.bit).sum();
    let reg_all: u32 = t.regions.iter().map(|n| n.bit).sum();

    // The title pool is exactly the concatenation of the record titles.
    let mut spans: Vec<(u32, u32)> =
        t.events.iter().chain(&t.items).map(|r| (r.title_offset, r.title_len)).collect();
    spans.sort();
    let mut next = 0;
    for (o, l) in spans {
        assert_eq!(o, next);
        next = o + l;
    }
    assert_eq!(next, t.sections.string_bytes);

    // Sequence numbers are unique across both tables, covering 1..=total.
    let seqs: HashSet<u32> = t.events.iter().chain(&t.items).map(|r| r.seq).collect();
    let total = (t.events.len() + t.items.len()) as u32;
    assert_eq!(seqs, (1..=total).collect());

    // Events: dated, sorted by start, end not before start, masks within the lists.
    let mut malformed = 0;
    let mut prev = None;
    for e in &t.events {
        let s = e.start.expect("event without a start date");
        assert!(matches!(e.kind, 1 | 3), "{}", e.title);
        assert!(e.importance <= 5);
        assert!(!e.title.is_empty());
        assert_eq!(u32::from(e.category_mask) & !cat_all, 0);
        assert_eq!(u32::from(e.region_mask) & !reg_all, 0);
        let key = (s.year, s.month, s.day);
        if let Some(p) = prev {
            assert!(key >= p, "{} out of order", e.title);
        }
        prev = Some(key);
        let ok_end = e.end.is_none_or(|end| end.year >= s.year);
        if !s.is_well_formed() || !e.end.is_none_or(|d| d.is_well_formed()) || !ok_end {
            malformed += 1;
        }
    }
    // Two known data-entry errors (Columbus end date, Havel start day).
    assert_eq!(malformed, 2);

    for i in &t.items {
        assert!(i.start.is_none() && i.end.is_none());
        assert_eq!((i.flags, i.kind, i.category_mask, i.region_mask), (0, 0, 0, 0));
    }
    assert_eq!(t.items.iter().filter(|i| !i.title.is_empty()).count(), 28);

    // Some well-known dates.
    let find = |title: &str| t.events.iter().find(|e| e.title == title).unwrap();
    let d = find("Battle of Hastings").start.unwrap();
    assert_eq!((d.year, d.month, d.day), (1066, 10, 14));
    let d = find("First Moon Landing").start.unwrap();
    assert_eq!((d.year, d.month, d.day), (1969, 7, 20));
    assert_eq!(find("Dinosaurs Become Extinct").start.unwrap().year, -65_000_000);
    assert_eq!(find("Solar Nebula Forms").start.unwrap().year, -4_700_000_000);

    // Term references are dense and point at events (allowing 3 stale ids).
    let event_ids: HashSet<u32> = t.events.iter().map(|e| e.ref_id).collect();
    let refs: usize = t.terms.iter().map(|k| k.ref_ids.len()).sum();
    assert_eq!(refs * 4, t.sections.term_ref_bytes as usize);
    let stale: HashSet<u32> =
        t.terms.iter().flat_map(|k| &k.ref_ids).filter(|r| !event_ids.contains(r)).copied().collect();
    assert!(stale.len() <= 3, "{stale:?}");
    assert!(t.terms.windows(2).all(|w| w[0].text_offset <= w[1].text_offset));

    // Every record refId exists in the catalog.
    let cat = root.join("eit/EE/ENCARTA/CATALOG.DLX/catalog2");
    if cat.join("content.ecn").exists() {
        let content = ecn_ids(&cat.join("content.ecn"));
        let data = ecn_ids(&cat.join("data.ecn"));
        for r in t.events.iter().chain(&t.items) {
            assert!(content.contains(&r.ref_id) && data.contains(&r.ref_id), "{}", r.ref_id);
        }
    }
}
