//! Whole-disc checks for the small formats (MindMaze, P2MTBL, MSWORLD.SDB, Geography Quiz,
//! `$WW$CTCATALOG`). Reads `ENCARTA_EXTRACTED` (default `<workspace>/extracted`); each test
//! skips when its files are absent. No disc bytes are stored here.

use encarta_formats::{itir_catalog, mindmaze, msworld, p2m};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

fn read(rel: &str) -> Option<Vec<u8>> {
    let p = root().join(rel);
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: {} not found", p.display());
            None
        }
    }
}

const ENC: &str = "iso/EE/ENCARTA";
const CATALOG2: &str = "eit/EE/ENCARTA/CATALOG.DLX/catalog2";

/// Ids of an ECN catalog file: `"ECN\0" | N × u32 location | N × u32 id | u32 N`.
fn ecn_ids(rel: &str) -> Option<HashSet<u32>> {
    let b = read(rel)?;
    assert_eq!(&b[..4], b"ECN\0");
    let n = u32::from_le_bytes(b[b.len() - 4..].try_into().unwrap()) as usize;
    assert_eq!(b.len(), 8 + 8 * n);
    Some((0..n).map(|i| u32::from_le_bytes(b[4 + 4 * n + 4 * i..8 + 4 * n + 4 * i].try_into().unwrap())).collect())
}

fn sdb() -> Option<msworld::Sdb> {
    Some(msworld::parse_sdb(&read(&format!("{ENC}/MSWORLD.SDB"))?).expect("SDB parses"))
}

fn p2m_table() -> Option<p2m::P2mTable> {
    Some(p2m::parse(&read(&format!("{ENC}/P2MTBL.DAT"))?).expect("P2MTBL parses"))
}

fn catalog(container: &str) -> Option<Vec<itir_catalog::CatalogEntry>> {
    let dir = format!("eit/EE/ENCARTA/{container}/$WW$CTCATALOG");
    let btree = read(&format!("{dir}/BTREE"))?;
    let data = read(&format!("{dir}/DATA"))?;
    Some(itir_catalog::scan(&btree, &data).expect("catalog scans"))
}

#[test]
fn mindmaze_whole_bank() {
    let (Some(db), Some(idx)) = (read(&format!("{ENC}/MINDMAZE.DB")), read(&format!("{ENC}/MINDMAZE.IDX"))) else { return };
    let qs = mindmaze::parse(&db, &idx).expect("MindMaze parses");
    assert_eq!(qs.len(), 8020);
    let mut per_group = [0usize; mindmaze::GROUP_COUNT];
    for q in &qs {
        per_group[q.group.unwrap() as usize] += 1;
        assert!(!q.clue.is_empty() && q.answers.iter().all(|a| !a.text.is_empty()));
        // the location word is shared by all answers
        assert!(q.answers.iter().all(|a| a.location == q.answers[0].location));
        assert!(q.answers.iter().all(|a| a.article_id / 1_000_000 == 761));
    }
    assert!(per_group.iter().all(|&n| n > 0));
    if let Some(content) = ecn_ids(&format!("{CATALOG2}/content.ecn")) {
        let refs: Vec<u32> = qs.iter().flat_map(|q| q.answers.iter().map(|a| a.article_id)).collect();
        let found = refs.iter().filter(|r| content.contains(r)).count();
        eprintln!("mindmaze: {found} of {} article refs in content.ecn", refs.len());
        assert!(found >= refs.len() - 100, "too many dead MindMaze article links");
    }
}

#[test]
fn p2m_whole_table() {
    let Some(t) = p2m_table() else { return };
    let links: usize = t.places.iter().map(|p| p.links.len()).sum();
    assert_eq!(t.places.len(), 12_253);
    assert_eq!(links, 16_122);
    assert!(t.places.iter().flat_map(|p| &p.links).all(|l| p2m::kind_label(l.kind) != "unknown"));
    if let Some(s) = sdb() {
        assert!(s.countries.iter().all(|c| t.links(c.id).is_some()), "every SDB country is a P2M place");
    }
    if let Some(content) = ecn_ids(&format!("{CATALOG2}/content.ecn")) {
        let articles: Vec<u32> =
            t.places.iter().flat_map(|p| &p.links).filter(|l| l.kind == 8).map(|l| l.content_id).collect();
        let found = articles.iter().filter(|r| content.contains(r)).count();
        eprintln!("p2m: {found} of {} article links in content.ecn", articles.len());
        assert!(found * 10 >= articles.len() * 7);
    }
    // the Kids copy is identical
    if let Some(kids) = read("iso/EE/KIDS/P2MTBL.DAT") {
        assert_eq!(p2m::parse(&kids).unwrap(), t);
    }
}

#[test]
fn sdb_whole_file() {
    let Some(s) = sdb() else { return };
    assert_eq!((s.countries.len(), s.categories.len(), s.qualifiers.len(), s.units.len()), (193, 12, 10, 575));
    assert_eq!(s.indicators.len(), 363);
    let countries: HashSet<u32> = s.countries.iter().map(|c| c.id).collect();
    let mut items = 0;
    for ind in &s.indicators {
        assert!(s.unit(ind.unit_id).is_some(), "indicator {} unit", ind.id);
        assert!(!s.category_names(ind.category_mask).is_empty());
        let mut seen = HashSet::new();
        for w in ind.items.windows(2) {
            assert!(w[0].value >= w[1].value, "indicator {} not sorted", ind.id);
        }
        for it in &ind.items {
            assert!(countries.contains(&it.country_id));
            assert!(seen.insert(it.country_id));
            let digits = it.value_text.split_once('.').map_or(0, |(_, f)| f.len());
            assert_eq!(digits, it.decimals as usize, "indicator {} value {}", ind.id, it.value_text);
        }
        items += ind.items.len();
    }
    assert_eq!(items, 57_347);
}

#[test]
fn geography_quiz() {
    let countries: Option<HashSet<u32>> = sdb().map(|s| s.countries.iter().map(|c| c.id).collect());
    let places: Option<HashSet<u32>> = p2m_table().map(|t| t.places.iter().map(|p| p.place_id).collect());
    for n in 1..=4 {
        let (Some(set), Some(pl)) = (read(&format!("{ENC}/D{n}.TXT")), read(&format!("{ENC}/D{n}PLACES.TXT"))) else {
            return;
        };
        let s = msworld::parse_quiz_settings(&set).unwrap();
        assert_eq!(s.points.len(), 5);
        assert!((s.type_mix.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        let rows = msworld::parse_quiz_places(&pl).unwrap();
        assert!(rows.iter().all(|r| r.kind <= 2 && r.extra == 0));
        if let Some(c) = &countries {
            // type 0 = SDB country; D4 has one stray row (a city, place 2989)
            let stray = rows.iter().filter(|r| r.kind == 0 && !c.contains(&r.entity)).count();
            assert!(stray <= 1, "D{n}: {stray} type-0 rows are not SDB countries");
        }
        if let Some(p) = &places {
            let found = rows.iter().filter(|r| p.contains(&r.entity)).count();
            eprintln!("D{n}PLACES: {} rows, {found} entities are P2M places", rows.len());
            assert!(found * 10 >= rows.len() * 9);
        }
    }
}

#[test]
fn msworld_media_catalog() {
    let Some(entries) = catalog("MSWORLD.ITR") else { return };
    assert_eq!(entries.len(), 5413);
    let media = msworld::media_records(&entries).unwrap();
    let mut kinds: HashMap<&str, usize> = HashMap::new();
    for m in &media {
        *kinds.entry(m.kind.as_str()).or_default() += 1;
        assert!(m.other.is_empty(), "media {} has unknown fields {:?}", m.key, m.other);
        assert!(m.place_id.is_some() && m.file.is_some() && m.credit.is_some() && !m.title.is_empty());
        assert_eq!(m.thumbnail.is_some(), m.kind != "w");
    }
    assert_eq!(kinds, HashMap::from([("i", 4775), ("w", 330), ("m", 308)]));
    let keys: HashSet<u32> = media.iter().map(|m| m.key).collect();
    let paired = media.iter().filter_map(|m| m.paired).filter(|u| keys.contains(u)).count();
    assert!(paired >= 650, "paired items resolve ({paired})");
    if let Some(t) = p2m_table() {
        let linked: HashSet<u32> = t.places.iter().flat_map(|p| &p.links).map(|l| l.content_id).collect();
        let found = keys.iter().filter(|k| linked.contains(k)).count();
        eprintln!("media: {found} of {} keys linked from P2MTBL", keys.len());
        assert!(found >= 3800);
    }
}

#[test]
fn timeline_catalog() {
    let Some(entries) = catalog("TIMELINE.EIT") else { return };
    assert_eq!(entries.len(), 22_749);
    assert!(entries.iter().enumerate().all(|(i, e)| e.key == i as u32 && e.properties.len() == 1));
}
