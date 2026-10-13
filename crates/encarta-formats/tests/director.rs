//! Whole-disc checks for the Director / Shockwave reader against `docs/formats/shockwave.md`.
//! Reads `$ENCARTA_EXTRACTED` (default `<workspace>/extracted`): the unpacked `SWSHARED.EIT` /
//! `SWALLDSK.EIT` packages under `eit/EE/{ENCARTA,KIDS}` and `iso/SUPPORT/DIRHTML`. Each test is
//! skipped when its files are absent.

use encarta_formats::catalog::{Catalog, Edition};
use encarta_formats::director::bitmap::BitmapInfo;
use encarta_formats::director::extract::{Extracted, extract_member};
use encarta_formats::director::interactive::{city_climate, population_chart};
use encarta_formats::director::locate::{director_files, find_interactive, resolve_with_catalog};
use encarta_formats::director::{DirectorFile, FileKind, MemberType, sound, text};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

fn edition_dir(kids: bool) -> Option<PathBuf> {
    let d = extracted().join(if kids {
        "eit/EE/KIDS"
    } else {
        "eit/EE/ENCARTA"
    });
    if d.join("SWALLDSK.EIT").is_dir() && d.join("SWSHARED.EIT").is_dir() {
        Some(d)
    } else {
        eprintln!(
            "skipping: no unpacked SWALLDSK.EIT/SWSHARED.EIT in {}",
            d.display()
        );
        None
    }
}

fn ext(p: &Path) -> String {
    p.extension()
        .map(|x| x.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn all_director_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for d in [extracted().join("eit"), extracted().join("iso/SUPPORT")] {
        v.extend(director_files(&d).into_iter().filter(|p| ext(p) != "swa"));
    }
    v
}

#[test]
fn every_director_file_opens_and_enumerates() {
    let Some(_) = edition_dir(false) else { return };
    let files = all_director_files();
    let mut by_ext: BTreeMap<String, usize> = BTreeMap::new();
    let (mut fields, mut bitmaps, mut scripts, mut sounds, mut empty_scripts) = (0, 0, 0, 0, 0);
    for p in &files {
        *by_ext.entry(ext(p)).or_default() += 1;
        let f = DirectorFile::parse(&std::fs::read(p).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e:#}", p.display()));
        let c = &f.container;
        assert_eq!(
            c.kind == FileKind::Movie,
            ext(p) == "dcr",
            "{}",
            p.display()
        );
        let cfg = f
            .config
            .as_ref()
            .unwrap_or_else(|| panic!("{}: no DRCF", p.display()));
        let diag = p.to_string_lossy().contains("DIRHTML");
        if diag {
            assert_eq!((c.versions.fver, cfg.director_version), (0x500, 1406));
        } else {
            assert_eq!(
                c.versions.label.as_deref(),
                Some("8.5.1#102"),
                "{}",
                p.display()
            );
            assert_eq!(cfg.director_version, 1850);
        }
        if c.kind == FileKind::Movie {
            assert!(
                f.casts.len() > 1,
                "{}: movie with no cast list",
                p.display()
            );
            if p.to_string_lossy().contains("/ia/") {
                assert_eq!(
                    (cfg.width(), cfg.height(), cfg.director_version),
                    (608, 424, 1850)
                );
            }
        }
        let internal: Vec<_> = f.casts.iter().filter(|c| !c.is_external()).collect();
        assert!(
            !internal.is_empty(),
            "{}: no cast with members",
            p.display()
        );
        for cast in internal {
            assert!(
                !cast.members.is_empty(),
                "{}: empty cast {}",
                p.display(),
                cast.name
            );
            for m in &cast.members {
                assert!(m.number >= cast.first_member && m.number <= cast.last_member);
                match m.kind {
                    MemberType::Field => {
                        let id = m.data_of("STXT").expect("field without STXT");
                        text::field_text(f.resource(id).unwrap()).unwrap();
                        fields += 1;
                    }
                    MemberType::Bitmap => {
                        BitmapInfo::parse(&m.specific).unwrap();
                        if m.data_of("BITD").is_some() || m.data_of("ediM").is_some() {
                            match extract_member(&f, cast, m, &Vec::new()) {
                                Ok(Extracted::Png(_) | Extracted::Jpeg(_)) => bitmaps += 1,
                                Ok(_) => panic!("bitmap gave non-image"),
                                Err(e) => panic!("{} member {}: {e:#}", p.display(), m.number),
                            }
                        }
                    }
                    MemberType::Script => {
                        // four scripts on the disc are empty (`empty class`, three in LOC_vwr)
                        if cast.handler_names(c, m).is_empty() {
                            empty_scripts += 1;
                        }
                        scripts += 1;
                    }
                    MemberType::Sound => {
                        let Ok(Extracted::Mp3(b)) = extract_member(&f, cast, m, &Vec::new()) else {
                            panic!("{} sound {}", p.display(), m.number)
                        };
                        assert!(sound::find_stream(&b, 3).is_some_and(|s| s.offset == 0));
                        sounds += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    eprintln!("{by_ext:?}: {fields} fields, {bitmaps} bitmaps, {scripts} scripts, {sounds} sounds");
    assert!(
        empty_scripts <= 8,
        "{empty_scripts} scripts without handlers"
    );
    if edition_dir(true).is_some() {
        assert_eq!(empty_scripts, 8);
        assert_eq!(by_ext.get("dcr"), Some(&7));
        assert_eq!(by_ext.get("cct"), Some(&64));
    }
}

/// The Kids packages hold byte-identical copies, and the catalog maps every file's `dswmedia/…`
/// key to the package it is in.
#[test]
fn kids_copies_and_catalog_keys() {
    let (Some(ee), Some(kids)) = (edition_dir(false), edition_dir(true)) else {
        return;
    };
    let files = director_files(&ee);
    assert!(files.len() >= 37);
    for p in &files {
        let rel = p.strip_prefix(&ee).unwrap();
        assert_eq!(
            std::fs::read(p).unwrap(),
            std::fs::read(kids.join(rel)).unwrap(),
            "{}",
            rel.display()
        );
    }
    let paths = Edition::Deluxe.paths(&extracted());
    if !paths.catalog2.join("generic.ecs").exists() || !paths.ini.exists() {
        eprintln!("skipping catalog part: no catalog");
        return;
    }
    let cat = Catalog::open(&paths.catalog2, &paths.ini).unwrap();
    for p in &files {
        let rel = p
            .strip_prefix(&ee)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let key = rel.split_once('/').unwrap().1; // drop the package folder
        let url = format!("msencdata://~catalog~/{key}");
        let found = resolve_with_catalog(&cat, std::slice::from_ref(&ee), &url)
            .unwrap_or_else(|| panic!("{url} does not resolve through generic.ecs"));
        assert_eq!(&found, p);
    }
}

#[test]
fn city_climate_dataset() {
    for kids in [false, true] {
        let Some(d) = edition_dir(kids) else { continue };
        let dir = find_interactive(&d.join("SWALLDSK.EIT"), "clim_CCL").unwrap();
        let (c, issues) = city_climate(&dir).unwrap();
        assert!(issues.is_empty(), "{issues:#?}");
        assert_eq!(c.title, "City Climate");
        assert_eq!(c.unit_systems, ["imperial", "metric"]);
        assert_eq!(c.default_units, "imperial");
        assert_eq!(c.max_selection, 8);
        assert_eq!(c.categories.len(), 2);
        assert_eq!(c.measures.len(), 8);
        assert_eq!(c.cities.len(), 165);
        assert_eq!(c.launch_filter.len(), 227);
        assert_eq!(
            c.launch_filter.iter().filter(|f| f.city.is_some()).count(),
            164
        );
        assert!(
            c.launch_filter
                .iter()
                .all(|f| f.city.is_none() || f.city_label.is_some())
        );
        assert_eq!(
            c.strings.not_available.as_deref(),
            Some("na = Data not available")
        );
        assert!(
            c.narration
                .as_ref()
                .is_some_and(|n| (n.duration_secs - 8.7).abs() < 0.1)
        );
        let mut nulls = 0;
        for city in &c.cities {
            for u in ["imperial", "metric"] {
                let v = &city.values[u];
                assert_eq!(v.len(), 8, "{}", city.label);
                nulls += v.values().filter(|x| x.is_null()).count();
                assert!(v.values().all(|x| x.is_null() || x.is_number()));
            }
        }
        assert!(nulls < 165 * 16 / 4, "{nulls} missing values");
        let albany = c
            .cities
            .iter()
            .find(|x| x.label == "Albany, United States")
            .unwrap();
        assert_eq!(
            albany.values["metric"]["temperature.january_low"],
            serde_json::json!(-11.7)
        );
        let bogota = c.cities.iter().find(|x| x.name == "Bogotá").unwrap();
        assert_eq!(bogota.country, "Colombia");
    }
}

#[test]
fn population_chart_dataset() {
    for kids in [false, true] {
        let Some(d) = edition_dir(kids) else { continue };
        let dir = find_interactive(&d, "pop_CPO")
            .unwrap_or_else(|| find_interactive(&d.join("SWALLDSK.EIT"), "pop_CPO").unwrap());
        let (p, issues) = population_chart(&dir).unwrap();
        assert!(issues.is_empty(), "{issues:#?}");
        assert_eq!(p.title, "Population Chart");
        assert_eq!(p.max_selection, 4);
        assert_eq!(p.launch_filter.len(), 64);
        let [us, ca] = &p.datasets[..] else {
            panic!("two datasets")
        };
        assert_eq!((us.regions.len(), ca.regions.len()), (51, 13));
        assert_eq!(us.years, (1800..=2000).step_by(20).collect::<Vec<_>>());
        assert_eq!(ca.years, (1861..=2001).step_by(20).collect::<Vec<_>>());
        assert_eq!(
            (us.default_units.as_str(), ca.default_units.as_str()),
            ("imperial", "metric")
        );
        for d in [us, ca] {
            let ids: Vec<&str> = d.measures.iter().map(|m| m.id.as_str()).collect();
            assert_eq!(ids, ["total", "density", "urban"]);
            for r in &d.regions {
                for u in ["imperial", "metric"] {
                    for (m, series) in &r.values[u] {
                        assert_eq!(series.len(), d.years.len(), "{} {u} {m}", r.name);
                    }
                    // the latest census is present for every region
                    assert!(r.values[u]["total"].last().unwrap().is_u64(), "{}", r.name);
                }
            }
        }
        let dc = us
            .regions
            .iter()
            .find(|r| r.name == "District of Columbia")
            .unwrap();
        assert!(dc.values["imperial"]["total"].iter().any(|v| v.is_u64()));
        assert!(ca.footnote.contains("Statistics Canada"));
        assert!(
            p.narration
                .as_ref()
                .is_some_and(|n| (n.duration_secs - 8.6).abs() < 0.1)
        );
    }
}

#[test]
fn swa_files_yield_mp3() {
    let mut n = 0;
    for d in [extracted().join("eit"), extracted().join("iso/SUPPORT")] {
        for p in director_files(&d).into_iter().filter(|p| ext(p) == "swa") {
            let b = std::fs::read(&p).unwrap();
            let (mp3, hdr, s) =
                sound::swa_to_mp3(&b).unwrap_or_else(|e| panic!("{}: {e:#}", p.display()));
            assert_eq!((hdr.sample_rate, hdr.bit_rate), (44100, 48000));
            assert_eq!(
                (
                    s.first.version,
                    s.first.layer,
                    s.first.bitrate_kbps,
                    s.first.mono
                ),
                (1, 3, 48, true)
            );
            // the frames run to (nearly) the end of the file
            assert!(
                mp3.len() - s.len < 1024,
                "{}: {} bytes after the last frame",
                p.display(),
                mp3.len() - s.len
            );
            assert!(s.duration_secs > 8.0 && s.duration_secs < 14.0);
            n += 1;
        }
    }
    if n == 0 {
        eprintln!("skipping: no .swa files");
    } else if edition_dir(true).is_some() {
        assert_eq!(n, 5);
    }
}
