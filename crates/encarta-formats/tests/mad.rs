//! Whole-disc checks for the MAD atlas database (docs/formats/mad.md §0.2 and the self-check
//! values of §4.5, §5.8, §7.8, §8.6). Reads `ENCARTA_EXTRACTED` (default `<workspace>/extracted`);
//! each test skips when the MAD files are absent. No disc bytes are stored here.

use encarta_formats::mad::gazetteer::Gazetteer;
use encarta_formats::mad::rmap;
use encarta_formats::mad::{Atlas, Edition, check};
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

fn atlas(e: Edition) -> Option<Atlas> {
    match Atlas::locate(&root(), e) {
        Ok(paths) => Some(Atlas::open(&paths).expect("MAD files open")),
        Err(err) => {
            eprintln!("skipping: {err}");
            None
        }
    }
}

fn run(e: Edition) -> Option<check::Report> {
    let a = atlas(e)?;
    let rep = check::run(&a, Some(e));
    for l in &rep.log {
        eprintln!("{l}");
    }
    assert!(rep.failures.is_empty(), "failures: {:#?}", rep.failures);
    Some(rep)
}

#[test]
fn world_database_matches_the_spec() {
    let Some(rep) = run(Edition::World) else { return };
    let v = |k: &str| rep.get(k).unwrap_or_else(|| panic!("no value {k}"));
    assert_eq!(v("RMAP tiles"), 357_792);
    assert_eq!(v("RMAP fill tiles"), 188_702);
    assert_eq!(v("GeoGem records decoded"), 217_323);
    assert_eq!(v("GeoGem keys"), 2_754_023);
    assert_eq!(v("GeoGem points"), 54_448_380);
    assert_eq!(v("GeoGem object references"), 2_947_064);
    assert_eq!(v("GeoGem references resolving into GeoObjGrid0409"), 2_947_064);
    assert_eq!(v("GeoObj distinct entities"), 1_813_920);
    assert_eq!(v("EntityConv entries matching GeoObj"), 1_813_920);
    assert_eq!(v("Find Grid0409 distinct entities"), 1_798_103);
    assert_eq!(v("EntityCenter distinct entities"), 2_015);
    assert_eq!(v("GeoObjGrid0409 records decoded"), 11_808);
    assert_eq!(v("EntityConv records decoded"), 1_772);
}

#[test]
fn student_database_matches_the_spec() {
    let Some(rep) = run(Edition::Student) else { return };
    let v = |k: &str| rep.get(k).unwrap_or_else(|| panic!("no value {k}"));
    assert_eq!(v("RMAP tiles"), 87_384);
    assert_eq!(v("RMAP fill tiles"), 68_526);
    assert_eq!(v("GeoGem records decoded"), 20_491);
    assert_eq!(v("GeoGem object references"), 166_539);
}

#[test]
fn whole_disc_tile_total() {
    let (Some(w), Some(s)) = (atlas(Edition::World), atlas(Edition::Student)) else { return };
    let count = |a: &Atlas| a.terrain().unwrap().rasters.iter().map(|r| r.index.key_count()).sum::<u64>();
    assert_eq!(count(&w) + count(&s), 445_176);
}

#[test]
fn aruba_is_entity_one() {
    let Some(a) = atlas(Edition::World) else { return };
    let gz = Gazetteer::open(&a, 0x409).unwrap();
    let e = gz.entity(1).unwrap();
    assert_eq!(e.names[0], "Aruba (Neth.)");
    let (lat, lon) = e.centroid.unwrap();
    assert!((lat - 12.50).abs() < 0.01 && (lon + 69.94).abs() < 0.01, "{lat} {lon}");
    assert!(e.search_keys.iter().any(|k| k.contains("Aruba")), "{:?}", e.search_keys);
}

#[test]
fn raster_orientation() {
    // The level-2 HYPSO mosaic: the pixel at a point in the Sahara is land and one in the
    // middle of the Pacific is water (CMT kind 0 = water, 2 = land including ice, in HYPSO).
    let Some(a) = atlas(Edition::World) else { return };
    let t = a.terrain().unwrap();
    let r = t.raster("HYPSO").unwrap();
    let side = rmap::image_side(2);
    let img = r.assemble(2).unwrap();
    let look = r.look("HYPSO").unwrap();
    let kind = |lat: f64, lon: f64| {
        let (row, col) = encarta_formats::mad::octahedral::pixel_of(lat, lon, side as u32);
        look.values[img[row as usize * side + col as usize] as usize].kind
    };
    assert_eq!(kind(23.0, 10.0), 2, "Sahara is land");
    assert_eq!(kind(-55.0, 132.0), 0, "Southern Ocean is water");
    assert_eq!(kind(0.0, -140.0), 0, "Pacific is water");
    assert_eq!(kind(75.0, -40.0), 2, "Greenland ice counts as land");
    assert_eq!(kind(-2.0, -60.0), 2, "Amazon is land");
}
