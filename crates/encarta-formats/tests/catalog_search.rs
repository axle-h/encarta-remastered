//! Whole-disc checks for the catalog and search readers.
//! Reads `$ENCARTA_EXTRACTED` (default `<workspace>/extracted`); each test is skipped when its
//! files are absent. The Deluxe postings scan is slow in debug builds; run with `--release`.

use encarta_formats::catalog::{Catalog, Edition, Table, check_package_files};
use encarta_formats::search::*;
use std::collections::HashMap;
use std::path::PathBuf;

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

fn open_catalog(ed: Edition) -> Option<(Catalog, Vec<PathBuf>)> {
    let p = ed.paths(&extracted());
    if !p.catalog2.join("guid.lst").exists() || !p.ini.exists() {
        eprintln!("skipping: {} not present", p.catalog2.display());
        return None;
    }
    Some((Catalog::open(&p.catalog2, &p.ini).unwrap(), p.package_dirs))
}

/// entries per package file name in one table
fn per_file(cat: &Catalog, t: Table) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for u in cat.package_usage().into_iter().filter(|u| u.table == t) {
        *m.entry(u.file.unwrap()).or_default() += u.count;
    }
    m
}

fn common_catalog_checks(cat: &Catalog, dirs: &[PathBuf]) {
    assert!(cat.validate().is_empty(), "{:?}", cat.validate());
    for (p, n, bound, distinct) in cat.package_extents() {
        assert!(
            n == bound as usize && n == distinct,
            "package {p} not dense: {n} {bound} {distinct}"
        );
    }
    check_package_files(cat, dirs).unwrap();
}

#[test]
fn catalog_deluxe() {
    let Some((cat, dirs)) = open_catalog(Edition::Deluxe) else {
        return;
    };
    assert_eq!(cat.packages.len(), 91);
    assert_eq!(
        [
            cat.content.len(),
            cat.data.len(),
            cat.relart.len(),
            cat.baggage.len(),
            cat.generic.len()
        ],
        [114_004, 477_393, 35_576, 65_892, 6_408]
    );
    assert_eq!((cat.baggage.width, cat.generic.width), (17, 72));
    common_catalog_checks(&cat, &dirs);

    let content = per_file(&cat, Table::Content);
    assert_eq!(
        (content["ContDlx.akc"], content["ContStd.akc"]),
        (69_949, 44_055)
    );
    let relart = per_file(&cat, Table::Relart);
    assert_eq!((relart["RelStd.akc"], relart["RelDlx.akc"]), (34_606, 970));
    assert_eq!(per_file(&cat, Table::Data)["DataDlx.akc"], 175_793);

    let zebra: Vec<_> = cat
        .resolve_id(761_575_094)
        .into_iter()
        .map(|r| (r.table, r.file.unwrap(), r.local_id))
        .collect();
    assert_eq!(
        zebra,
        [
            (Table::Content, "ContStd.akc".to_string(), 22_865),
            (Table::Data, "DataStd.akc".to_string(), 81_969),
            (Table::Relart, "RelStd.akc".to_string(), 18_691),
        ]
    );
    let r = cat.resolve_key("0000EA10.gsm");
    assert_eq!(r[0].file.as_deref(), Some("PiconStd.eit"));
    assert!(cat.resolve_id(1).is_empty());
}

#[test]
fn catalog_kids() {
    let Some((cat, dirs)) = open_catalog(Edition::Kids) else {
        return;
    };
    assert_eq!(
        [
            cat.content.len(),
            cat.data.len(),
            cat.relart.len(),
            cat.baggage.len(),
            cat.generic.len()
        ],
        [637, 5_974, 663, 9_905, 6_671]
    );
    common_catalog_checks(&cat, &dirs);
    assert_eq!(per_file(&cat, Table::Content)["ContKdc.akc"], 637);
}

struct Expect {
    pfx: [usize; 2],
    words: usize,
    word_ids: usize,
    live: usize,
    walk: usize,
}

fn check_search(tla: &str, e: Expect) -> Option<()> {
    let dir = extracted().join("finddlx");
    let Ok(files) = FindFiles::locate(&dir, tla) else {
        eprintln!("skipping: {tla} search files not in {}", dir.display());
        return None;
    };
    let pfx = Pfx::parse(&std::fs::read(&files.pfx).unwrap()).unwrap();
    assert_eq!(
        pfx.collections
            .iter()
            .map(|c| c.entries.len())
            .collect::<Vec<_>>(),
        e.pfx
    );
    assert!(pfx.collections.iter().all(|c| c.lcid == 0x409));

    let cm = CharMap::parse(&std::fs::read(&files.idxcm).unwrap()).unwrap();
    assert_eq!((cm.culture.as_str(), cm.entries.len()), ("en-US", 383));

    let wb = std::fs::read(&files.idxw).unwrap();
    let (words, _) = IdxW::new(&wb).unwrap().walk().unwrap();
    assert_eq!(words.len(), e.words);
    assert!(
        words.windows(2).all(|w| w[0].key < w[1].key),
        "B-tree keys not in ordinal order"
    );
    let word_ids = distinct_word_ids(&words);
    assert_eq!(word_ids.len(), e.word_ids);

    // Postings blocks tile the file back to back from the header to EOF.
    let eb = std::fs::read(&files.idxew).unwrap();
    let ew = IdxEw::new(&eb).unwrap();
    let mut expect = ew.data_start;
    for &w in &word_ids {
        assert_eq!(w as usize, expect, "postings block not contiguous");
        expect = ew.postings_at(w).unwrap().1;
    }
    assert_eq!(expect, eb.len());

    let xb = std::fs::read(&files.idxe).unwrap();
    let idxe = IdxE::new(&xb).unwrap();
    let live = live_entry_ids(&ew, &word_ids).unwrap();
    assert_eq!(live.len(), e.live);
    for &id in &live {
        idxe.record(id).unwrap();
    }
    let mut walk = idxe.walk();
    let walked: Vec<u32> = walk.by_ref().map(|r| r.entry_id).collect();
    assert_eq!(
        (walked.len(), walk.failures, walk.last_end),
        (e.walk, 0, xb.len())
    );
    // every live record lies on the walk path
    let mut i = 0;
    for id in &live {
        while walked[i] < *id {
            i += 1;
        }
        assert_eq!(walked[i], *id);
    }

    if tla == "DLX" {
        let zebra = words.iter().find(|w| w.key == "zebra").unwrap();
        let alts = ew.postings(zebra.word_id).unwrap();
        let hit = alts
            .iter()
            .flat_map(|a| &a.t1)
            .find(|h| h.ref_id == 761_575_094)
            .expect("Zebra title hit");
        assert_eq!(idxe.record(hit.entry_id).unwrap().display_text, "Zebra");
    }
    Some(())
}

#[test]
fn search_kids() {
    check_search(
        "KDC",
        Expect {
            pfx: [5_749, 8_333],
            words: 55_261,
            word_ids: 17_809,
            live: 50_257,
            walk: 50_257,
        },
    );
}

#[test]
fn search_deluxe() {
    check_search(
        "DLX",
        Expect {
            pfx: [198_701, 289_044],
            words: 1_120_732,
            word_ids: 984_105,
            live: 3_034_468,
            walk: 3_034_472,
        },
    );
}
