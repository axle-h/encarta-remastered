//! Whole-disc checks for the AKC reader against `docs/formats/akc.md` §3.4, §10 and §11.
//! AKC files are read from `$ENCARTA_ISO/EE/…` when `ENCARTA_ISO` is an unpacked disc directory,
//! else from `$ENCARTA_EXTRACTED/iso/EE/…` (default `<workspace>/extracted`). Tests are skipped
//! when the files are absent. Decoding everything is slow in debug builds; use `--release`.

use encarta_formats::akc::check::{self, Options, SPEC_FILES, SPEC_RECORDS, hex};
use encarta_formats::akc::{AkcFile, CodeStats, Library};
use encarta_formats::catalog::{Edition, Table};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

fn disc_root() -> PathBuf {
    match std::env::var_os("ENCARTA_ISO").map(PathBuf::from) {
        Some(p) if p.is_dir() => p,
        _ => extracted().join("iso"),
    }
}

/// Edition directory (`EE/ENCARTA` or `EE/KIDS`) if present.
fn dir(kids: bool) -> Option<PathBuf> {
    let d = disc_root().join(if kids { "EE/KIDS" } else { "EE/ENCARTA" });
    if d.join(if kids { "CONTKDC.AKC" } else { "CONTSTD.AKC" })
        .exists()
    {
        Some(d)
    } else {
        eprintln!("skipping: no AKC files in {}", d.display());
        None
    }
}

fn is_kids(name: &str) -> bool {
    name.contains("KDC")
}

fn open(name: &str) -> Option<AkcFile> {
    let d = dir(is_kids(name))?;
    Some(AkcFile::open(d.join(name)).unwrap())
}

/// §3.4: N, direct records, block entries M, blocks, key width.
const INDEX: &[(&str, usize, usize, usize, usize, u32)] = &[
    ("CONTDLX.AKC", 69_949, 26, 955, 981, 1),
    ("CONTSTD.AKC", 44_055, 42, 522, 564, 1),
    ("DATADLX.AKC", 175_793, 1, 505, 506, 1),
    ("DATASTD.AKC", 121_055, 1, 447, 448, 1),
    ("DATAFDLX.AKC", 28_112, 106, 740, 846, 1),
    ("DATAFSTD.AKC", 152_433, 425, 2_831, 3_256, 1),
    ("RELDLX.AKC", 970, 2, 14, 16, 1),
    ("RELSTD.AKC", 34_606, 485, 981, 1_466, 1),
    ("DATAESK.AKC", 1, 1, 0, 1, 0),
    ("CONTKDC.AKC", 637, 0, 12, 12, 1),
    ("DATAKDC.AKC", 5_874, 0, 24, 24, 1),
    ("DATAFKDC.AKC", 100, 0, 1, 1, 1),
    ("RELKDC.AKC", 663, 0, 3, 3, 1),
];

#[test]
fn headers_and_stubs() {
    let (Some(enc), Some(kids)) = (dir(false), dir(true)) else {
        return;
    };
    let mut stubs = Vec::new();
    let mut count = 0;
    for d in [enc, kids] {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("akc")) {
                continue;
            }
            count += 1;
            let f = AkcFile::open(&p).unwrap();
            let h = f.header();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                h.label
                    .to_ascii_uppercase()
                    .contains(&format!("GUID='{}'", h.guid_string())),
                "{name}: label names its GUID"
            );
            if h.is_stub() {
                assert_eq!(
                    (h.dict_offset, h.dict_len, h.index_offset, h.index_len),
                    (0, 0, 0, 0)
                );
                assert!(f.is_empty());
                stubs.push(name);
            } else {
                assert_eq!(h.flags, 0);
                let label_len = h.label.encode_utf16().count() as u32 * 2 + 2; // + BOM
                assert_eq!(h.dict_offset, 0x30 + label_len, "{name}");
                assert_eq!(
                    (h.index_offset + h.index_len) as u64,
                    std::fs::metadata(&p).unwrap().len(),
                    "{name}: index is last"
                );
                let s = f.shared().unwrap();
                assert!(s.capitalize && s.self_closing, "{name}");
                assert_eq!(s.header.x, 0);
                assert!(s.trailing_bits.0 < 16 && s.trailing_bits.1, "{name}");
            }
        }
    }
    stubs.sort();
    assert_eq!(count, 22);
    assert_eq!(
        stubs,
        [
            "CONTECK.AKC",
            "CONTEDK.AKC",
            "CONTEDK.AKC",
            "CONTESK.AKC",
            "CONTESK.AKC",
            "DATAECK.AKC",
            "DATAEDK.AKC",
            "DATAEDK.AKC"
        ]
    );
}

#[test]
fn index_shapes() {
    for &(name, n, direct, m, blocks, kw) in INDEX {
        let Some(f) = open(name) else { return };
        let got_direct = f.blocks().iter().filter(|b| b.direct).count();
        assert_eq!(
            (f.len(), got_direct, f.block_entries(), f.blocks().len()),
            (n, direct, m, blocks),
            "{name}"
        );
        assert_eq!(f.index_header().unwrap().key_width, kw, "{name}");
        assert_eq!(f.sparse_entries(), (n + 30) / 32 + 1, "{name}");
        // keys are dense
        assert!(
            f.records()
                .iter()
                .enumerate()
                .all(|(i, r)| r.key as usize == i)
        );
    }
}

#[test]
fn worked_examples() {
    let Some(f) = open("CONTSTD.AKC") else { return };
    let ih = f.index_header().unwrap();
    let d: Vec<_> = ih.direct.iter().map(|x| (x.base, x.width, x.max)).collect();
    assert_eq!(d[1], (132_321, 19, 545_732));
    assert_eq!(d[3], (31_791_298, 22, 33_921_932));
    let s: Vec<_> = ih.shared.iter().map(|x| (x.base, x.width, x.max)).collect();
    assert_eq!(s[0], (1, 10, 522));
    assert_eq!(s[3], (97_272, 25, 31_729_648));
    let loc = |k: u32| {
        let r = f.record_info(k).unwrap();
        let b = &f.blocks()[r.block as usize];
        (b.offset, b.clen, r.ulen, r.ntok, r.start)
    };
    assert_eq!(loc(0), (14_539_720, 64_764, 5_242, 950, 35_960));
    assert_eq!(loc(1), (1_155_716, 57_210, 814, 162, 16_331));
    assert_eq!(loc(22_865), (20_011_212, 63_128, 16_503, 3_045, 0));
    let sh = f.shared().unwrap();
    let h = sh.header;
    assert_eq!(
        (h.a, h.wc, h.total, h.maxlen),
        (13_084_134, 35_510, 262_141, 30)
    );
    assert_eq!(sh.trailing_bits.0, 0);

    let Some(k) = open("DATAFKDC.AKC") else {
        return;
    };
    let r = *k.record_info(37).unwrap();
    assert_eq!((r.start, r.ntok, r.ulen), (0, 45, 280));
    let b = &k.blocks()[r.block as usize];
    assert_eq!((b.offset, b.clen), (340, 3_364));
    let d = k.decode_block(r.block).unwrap();
    let h = d.dict.header;
    assert_eq!(
        (h.a, h.x, h.wc, h.total, h.maxlen),
        (495, 0, 365, 2_593, 16)
    );
    let text = k.record(37).unwrap();
    assert!(text.starts_with(b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n"));
    assert!(text.ends_with(b"</data>"));
}

#[test]
fn spec_record_hashes() {
    for &(name, key, ulen, sha) in SPEC_RECORDS {
        let Some(f) = open(name) else { return };
        let b = f.record(key).unwrap();
        assert_eq!(b.len() as u64, ulen, "{name} {key}");
        assert_eq!(hex(&Sha256::digest(&b)), sha, "{name} {key}");
    }
}

/// Decodes every record of every package and compares with every §11 value.
#[test]
fn whole_disc() {
    let (Some(_), Some(_)) = (dir(false), dir(true)) else {
        return;
    };
    let mut codes = CodeStats::default();
    let mut records = 0;
    let mut bytes = 0;
    let mut checked = 0;
    for &(name, n, total, largest, agg) in SPEC_FILES {
        let copies: &[bool] = if name == "DATAESK.AKC" {
            &[false, true]
        } else {
            &[is_kids(name)]
        };
        for &kids in copies {
            let f = AkcFile::open(dir(kids).unwrap().join(name)).unwrap();
            let rep = check::check_file(&f, &Options::default());
            assert_eq!(rep.error_count, 0, "{name}: {:?}", rep.errors);
            assert_eq!((rep.xml_ok, rep.non_utf8), (n, 0), "{name}");
            assert_eq!(
                (rep.records, rep.bytes, rep.largest),
                (n, total, largest),
                "{name}"
            );
            assert_eq!(hex(&rep.aggregate), agg, "{name}");
            assert_eq!(check::compare_with_spec(name, &rep, &f), Some(vec![]));
            // Every token emits at least one byte; records' ntok sum to the decoded tokens.
            assert!(f.records().iter().all(|r| r.ntok <= r.ulen));
            codes.add(&rep.codes);
            codes.add(&f.shared().unwrap().stats);
            records += rep.records;
            bytes += rep.bytes;
            checked += 1;
        }
    }
    assert_eq!(checked, 14);
    assert_eq!((records, bytes), (634_249, 2_136_740_166)); // DataESK counted twice
    // §5.2: 32,296 pre-tree, 4 sparse, 0 flat; §6.1: 256 reuse bits (prefix/even/odd).
    assert_eq!(
        (codes.pretree, codes.sparse, codes.flat, codes.reused),
        (32_296, 4, 0, 256)
    );
}

/// Every content/data/relart catalog entry decodes, and its root `refid` is the entry's id.
fn catalog_cross_check(edition: Edition) {
    let p = edition.paths(&extracted());
    if !p.catalog2.join("guid.lst").exists() || dir(edition == Edition::Kids).is_none() {
        eprintln!("skipping: catalog not present");
        return;
    }
    let lib = Library::open(&extracted(), edition).unwrap();
    let cat = lib.catalog();
    let mut n = 0;
    for t in [Table::Content, Table::Data, Table::Relart] {
        let mut by_pkg: std::collections::HashMap<u8, Vec<(u32, u32)>> = Default::default();
        for (id, loc) in cat.ecn(t).unwrap().iter() {
            by_pkg
                .entry(loc.package)
                .or_default()
                .push((loc.local_id, id));
        }
        for (pkg, entries) in by_pkg {
            let f = lib.package(pkg).unwrap();
            let rep = check::check_file(&f, &Options::default());
            assert_eq!(rep.error_count, 0);
            assert_eq!(entries.len(), f.len(), "every record is catalogued");
            for (local, id) in entries {
                let pos = f.position(local).unwrap();
                assert_eq!(rep.root_refids[pos], Some(id), "{t:?} {id}");
                n += 1;
            }
        }
    }
    assert_eq!(
        n,
        if edition == Edition::Kids {
            7_274
        } else {
            626_973
        }
    );
}

#[test]
fn catalog_deluxe() {
    catalog_cross_check(Edition::Deluxe);
    let Ok(lib) = Library::open(&extracted(), Edition::Deluxe) else {
        return;
    };
    let sha = |b: &[u8]| hex(&Sha256::digest(b));
    let c = lib.content(761_575_094).unwrap().unwrap();
    assert_eq!((c.file.as_str(), c.local_id), ("ContStd.akc", 22_865));
    assert_eq!(
        sha(&c.bytes),
        "c91922f87c3114ce1412e70a3a50bec62212965ee7ac412305a12cb0941b182f"
    );
    let d = lib.data(761_575_094).unwrap().unwrap();
    assert_eq!(
        sha(&d.bytes),
        "7b43f187e62a5d7275d73db710f956a19929d6203d0b0e414f7c71aa5a6cad37"
    );
    let r = lib.related(761_561_691).unwrap().unwrap();
    assert_eq!(
        sha(&r.bytes),
        "bbd282210fb6cc0f92bb73064eca03282a9d14e5e93209ea526310a9249e7743"
    );
    assert!(lib.content(1).unwrap().is_none());
    let e = lib.get_key("list/expert.xml").unwrap().unwrap();
    assert_eq!(e.bytes.len(), 902_785);
    let text = encarta_formats::akc::xml_text(&c.bytes).unwrap();
    assert!(text.starts_with("<content refid=\"761575094\""));
}

#[test]
fn catalog_kids() {
    catalog_cross_check(Edition::Kids);
}
