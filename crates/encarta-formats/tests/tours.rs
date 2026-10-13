//! Whole-disc checks for the 3D virtual tours against the self-check values of
//! docs/formats/tours-3d.md. Reads `ENCARTA_EXTRACTED` (default `<workspace>/extracted`) and skips
//! when the tour containers are absent. No disc bytes are stored here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use encarta_formats::tours::script::Subsystem;
use encarta_formats::tours::textures::{ImageFormat, Resolution};
use encarta_formats::tours::tour::{FileClass, Issue, Tour, classify, is_hotspot_click};
use encarta_formats::tours::walkmap::{CellData, CellKind};
use encarta_formats::tours::{cloud, layer2d, viewpoints, walkmap, xfile};

const IDS: [&str; 7] = ["04257", "05285", "05916", "05917", "05918", "05919", "05920"];

fn world() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
        .join("eit/CONTENT/WORLD/7015")
}

/// All seven tours, loaded once; `None` when the data is absent.
fn tours() -> Option<&'static Vec<Tour>> {
    static TOURS: OnceLock<Option<Vec<Tour>>> = OnceLock::new();
    TOURS
        .get_or_init(|| {
            let w = world();
            if !w.join("04257.EIT/load.3sc").is_file() {
                eprintln!("skipping: {} not found", w.display());
                return None;
            }
            Some(IDS.iter().map(|id| Tour::load(&w.join(format!("{id}.EIT"))).unwrap()).collect())
        })
        .as_ref()
}

fn read(t: &Tour, f: &str) -> Vec<u8> {
    t.container.read(f).unwrap()
}

fn model(t: &Tour) -> &xfile::XFile {
    &t.model.as_ref().unwrap().data
}

#[test]
fn container_catalog() {
    let Some(tours) = tours() else { return };
    // §5.7 per-extension counts: jpg bmp gif cur x 3sc 3wm 3lt 32d 3cl 3vp log db psd.
    let expect: [[usize; 14]; 7] = [
        [36, 27, 16, 12, 1, 6, 1, 1, 2, 7, 0, 0, 0, 0],
        [83, 14, 5, 12, 1, 5, 7, 1, 2, 0, 0, 1, 0, 0],
        [37, 27, 4, 12, 1, 5, 9, 1, 2, 0, 1, 0, 1, 1],
        [63, 56, 4, 12, 1, 5, 2, 1, 2, 0, 0, 1, 0, 1],
        [90, 13, 4, 12, 1, 5, 11, 1, 2, 0, 1, 1, 1, 0],
        [108, 5, 4, 12, 1, 5, 1, 1, 2, 0, 1, 0, 0, 0],
        [73, 9, 4, 12, 1, 5, 13, 1, 2, 0, 0, 0, 1, 0],
    ];
    let exts = ["jpg", "bmp", "gif", "cur", "x", "3sc", "3wm", "3lt", "32d", "3cl", "3vp", "log", "db", "psd"];
    let mut total = 0;
    for (t, e) in tours.iter().zip(expect) {
        let files = &t.container.files;
        total += files.len();
        let got = exts.map(|x| files.iter().filter(|f| f.to_lowercase().ends_with(&format!(".{x}"))).count());
        assert_eq!(got, e, "{}", t.container.dir.display());
        assert_eq!(e.iter().sum::<usize>(), files.len(), "every file classified");
        assert!(files.iter().all(|f| classify(f) != FileClass::Unknown));
        // Every file parses: no structural issues, nothing unexplained.
        let structural: Vec<_> = t.issues.iter().filter(|i| i.is_structural()).collect();
        assert!(structural.is_empty(), "{structural:?}");
    }
    assert_eq!(total, 892);
}

#[test]
fn geometry_self_check() {
    let Some(tours) = tours() else { return };
    // §5.3: bytes, tokens, top-level, Material, TextureFilename, Mesh, vertices, faces, tri, quad,
    // fan triangles, max vertices in one mesh.
    let expect: [[usize; 12]; 7] = [
        [2_082_194, 39_357, 1_632, 89, 88, 1_542, 40_777, 20_059, 6_342, 13_717, 33_776, 1_694],
        [4_937_381, 93_221, 3_799, 109, 108, 3_689, 102_233, 41_956, 8_354, 33_602, 75_558, 1_716],
        [5_928_261, 59_249, 2_456, 134, 133, 2_321, 139_074, 59_640, 17_644, 41_996, 101_636, 2_951],
        [5_116_191, 37_545, 1_512, 167, 166, 1_344, 118_050, 52_269, 18_721, 33_548, 85_817, 1_260],
        [5_770_619, 79_217, 2_978, 134, 133, 2_843, 123_015, 63_108, 53_204, 9_904, 73_012, 908],
        [2_860_300, 14_333, 646, 114, 112, 531, 45_976, 42_477, 29_460, 13_017, 55_494, 5_054],
        [7_938_192, 184_585, 7_441, 125, 124, 7_315, 173_080, 63_513, 12_819, 50_694, 114_207, 582],
    ];
    // §5.3/5.4 extras: lod_ meshes, lod orphans, case-collision groups, degenerate faces,
    // non-finite UVs, merged material groups.
    let extras: [[usize; 6]; 7] =
        [[116, 0, 0, 4, 0, 10], [320, 0, 26, 0, 4, 11], [0, 0, 1, 1, 0, 23], [64, 1, 15, 0, 0, 27], [0, 0, 1, 0, 0, 28], [0, 0, 0, 0, 0, 4], [0, 0, 0, 0, 0, 17]];
    let (mut meshes, mut same_normals, mut single, mut per_face, mut multi, mut max_mats) = (0, 0, 0, 0, 0, 0);
    for ((t, e), x) in tours.iter().zip(expect).zip(extras) {
        let m = t.model.as_ref().unwrap();
        let s = m.data.stats();
        let size = read(t, &m.file).len();
        let got = [size, s.tokens, s.top_level_objects, s.materials, s.texture_filenames, s.meshes, s.vertices, s.faces, s.triangles_faces, s.quad_faces, s.fan_triangles, s.max_mesh_vertices];
        assert_eq!(got, e, "{}", m.file);
        assert_eq!(s.other_faces, 0);
        assert_eq!((s.with_normals, s.with_uvs, s.with_material_list), (s.meshes, s.meshes, s.meshes), "all four child templates");
        let got = [s.lod_meshes, s.lod_orphans, m.data.case_collision_groups(), s.degenerate_faces, s.non_finite_uvs, m.data.duplicate_material_groups()];
        assert_eq!(got, x, "{} extras", m.file);
        assert_eq!(m.data.header, [1, 0, 0]);
        assert_eq!(m.data.discarded_material.map(|i| m.data.materials[i].name.clone()), Some(Some("xof_default".into())));
        // Names unique when compared case-sensitively; all material references resolve.
        let names: BTreeSet<_> = m.data.meshes.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names.len(), m.data.meshes.len());
        assert!(m.data.materials.iter().all(|m| m.face_color[3] == 1.0), "alpha 1 everywhere");
        meshes += s.meshes;
        same_normals += s.normal_count_equals_vertex_count;
        single += s.single_material_lists;
        per_face += s.per_face_material_lists;
        multi += s.multi_material_lists;
        max_mats = max_mats.max(s.max_materials_per_mesh);
    }
    assert_eq!((meshes, same_normals, single, per_face, multi, max_mats), (19_585, 191, 14_659, 4_926, 1_330, 39));
    let per = model(&tours[3]);
    assert_eq!(per.lod_pairs().iter().filter(|(_, p)| p.is_none()).map(|(i, _)| per.meshes[*i].name.as_str()).collect::<Vec<_>>(), vec!["lod_strim16"]);
    let rom = model(&tours[1]);
    let (_, o2138) = rom.meshes.iter().enumerate().find(|(_, m)| m.name == "o2138").unwrap();
    assert_eq!(o2138.uvs.as_ref().unwrap().iter().flatten().filter(|v| !v.is_finite()).count(), 4);
    let (lo, hi) = model(&tours[0]).stats().bounds.unwrap();
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.06);
    assert!(close(lo, [-59.3, -5.7, -61.8]) && close(hi, [56.6, 31.0, 53.7]), "{lo:?} {hi:?}");
}

#[test]
fn textures_and_images() {
    let Some(tours) = tours() else { return };
    // §5.5: distinct names: exact / jpg fallback / missing.
    let expect = [[72, 0, 0], [91, 0, 0], [31, 29, 0], [106, 1, 1], [48, 38, 0], [95, 13, 0], [18, 66, 0]];
    let keyed = [18, 4, 21, 50, 10, 4, 6];
    let mut mismatched = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        let count = |r: Resolution| t.textures.iter().filter(|x| x.resolution == r).count();
        assert_eq!([count(Resolution::Exact), count(Resolution::JpgFallback), count(Resolution::Missing)], expect[i]);
        assert_eq!(count(Resolution::GifFallback), 0);
        assert_eq!(t.images.values().filter(|i| i.colour_keyed).count(), keyed[i]);
        // fireAnm.bmp is keyed wherever it exists (Persepolis has none).
        if let Some(f) = t.container.find("fireAnm.bmp") {
            assert!(t.images[f].colour_keyed);
        } else {
            assert_eq!(IDS[i], "05917");
        }
        for (name, info) in &t.images {
            let ext = name.rsplit('.').next().unwrap().to_lowercase();
            let sniffed = match info.format {
                ImageFormat::Bmp => "bmp",
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Gif => "gif",
                f => panic!("{name}: {f:?}"),
            };
            if ext != sniffed {
                mismatched.push(format!("{}/{name}", IDS[i]));
            }
            if !info.power_of_two() {
                assert!(["3DVTicon.bmp", "dir.gif", "dirTxt.gif", "loadtxt.gif"].iter().any(|h| h.eq_ignore_ascii_case(name)), "{name} not a power of two");
            }
        }
        // Every resolved 3D texture is a power of two between 16 and 1024.
        for r in &t.textures {
            if let Some(f) = &r.file {
                let info = &t.images[f];
                assert!(info.power_of_two() && (16..=1024).contains(&info.width) && (16..=1024).contains(&info.height), "{f}");
            }
        }
        let icon = &t.images[t.container.find("3DVTicon.bmp").unwrap()];
        assert_eq!((icon.width, icon.height, icon.bits_per_pixel), (420, 22, Some(24)));
        // Cursors: the same 12 in every tour, all 32×32.
        assert_eq!(t.cursors.len(), 12);
        assert!(t.cursors.values().all(|c| c.iter().all(|i| (i.width, i.height) == (32, 32))));
        assert_eq!(t.cursor_slots.len(), 7);
    }
    assert_eq!(mismatched, vec!["05285/ioban3.bmp", "05917/Kevin.jpg"]);
    let per = &tours[3];
    assert_eq!(per.textures.iter().find(|r| r.resolution == Resolution::Missing).unwrap().name, "roooof.jpg");
    assert_eq!(tours.iter().map(|t| t.images.len()).sum::<usize>(), 682);
}

#[test]
fn dangling_references_and_unused_files() {
    let Some(tours) = tours() else { return };
    for (i, t) in tours.iter().enumerate() {
        let mut dangling: Vec<String> = t
            .issues
            .iter()
            .filter(|i| i.is_dangling_file())
            .map(|i| match i {
                Issue::MissingFile { name, .. } => name.to_lowercase(),
                Issue::MissingTexture { name, .. } => name.to_lowercase(),
                _ => unreachable!(),
            })
            .collect();
        dangling.sort();
        let expect: Vec<&str> = if i == 3 { vec!["grip.cur", "roooof.jpg"] } else { vec!["grip.cur"] };
        assert_eq!(dangling, expect, "{}", IDS[i]);
    }
    // §5.7 images referenced by nothing (including the siblings of exact `X.bmp`/`X.jpg` matches).
    let unused: [&[&str]; 7] = [
        &["cloud91.jpg"],
        &["Blue_98.bmp", "bas_02.jpg", "basalt2.jpg", "sky_0105.bmp", "wai02.jpg"],
        &["Broun92.bmp", "Broun93.bmp", "First8.jpg", "Wall 9.jpg", "Water120.jpg", "Water121.jpg", "Water5.jpg"],
        &["Cynthia.bmp", "Kevin.jpg", "Nanae2.bmp", "adam1.bmp", "armor94.jpg", "k.bmp", "l3257.jpg", "l451a.jpg", "psurr01.bmp", "psurr02.bmp", "sky_009A.jpg"],
        &[
            "Cllnd007.jpg", "Cllnd018.jpg", "Hllnd001.jpg", "Hllnd002.jpg", "Hllnd003.jpg", "Hllnd004.jpg", "Hllnd005.jpg", "Hllnd006.jpg", "Hllnd008.jpg", "Hllnd020.jpg",
            "col_dot.bmp", "elev46.jpg", "elev4956.jpg", "fireAnm.bmp", "fireStrip.bmp", "hallw972.jpg", "sky_0105.jpg", "standin.bmp", "travnew.jpg",
        ],
        &["Blue_98.jpg", "abu_dot.bmp", "outside.jpg"],
        &["BLue_130.bmp", "Surro124.jpg", "Top_g68.jpg"],
    ];
    for (t, u) in tours.iter().zip(unused) {
        let images: Vec<&str> = t.unreferenced_files().into_iter().filter(|f| classify(f) == FileClass::Image).collect();
        assert_eq!(images, u.to_vec());
    }
}

/// §6.2 per-file table: bytes, cells, polys, ground, hotspot, collision, visibility.
const WALKMAPS: &[(&str, &str, [usize; 7])] = &[
    ("04257", "acr.3wm", [955_538, 1713, 3465, 1055, 237, 16, 405]),
    ("05285", "1stFloor.3wm", [754_320, 673, 1090, 372, 109, 3, 189]),
    ("05285", "2ndFloor.3wm", [141_679, 225, 377, 143, 8, 3, 71]),
    ("05285", "3rdFloor.3wm", [70_930, 49, 77, 23, 7, 1, 18]),
    ("05285", "stair1.3wm", [9_943, 35, 141, 27, 6, 1, 1]),
    ("05285", "stair2.3wm", [5_464, 21, 45, 13, 6, 1, 1]),
    ("05285", "stair3.3wm", [13_742, 41, 119, 30, 6, 1, 4]),
    ("05285", "stair4.3wm", [5_286, 25, 61, 17, 6, 1, 1]),
    ("05916", "level1.3wm", [256_273, 856, 1652, 714, 68, 2, 72]),
    ("05916", "level2.3wm", [363_525, 1455, 2820, 1244, 110, 2, 99]),
    ("05916", "level3.3wm", [343_189, 995, 1895, 828, 71, 2, 94]),
    ("05916", "level4.3wm", [81_540, 362, 689, 302, 14, 1, 45]),
    ("05916", "level5.3wm", [84_296, 387, 784, 368, 8, 1, 10]),
    ("05916", "stair2.3wm", [12_267, 59, 213, 41, 13, 1, 4]),
    ("05916", "stair3.3wm", [21_485, 92, 306, 63, 24, 1, 4]),
    ("05916", "stair4.3wm", [11_168, 52, 174, 35, 12, 1, 4]),
    ("05916", "stair5.3wm", [10_673, 50, 173, 35, 12, 1, 2]),
    ("05917", "per.3wm", [925_691, 1254, 1838, 818, 190, 1, 245]),
    ("05917", "stair1.3wm", [928_096, 1257, 1842, 819, 191, 1, 246]),
    ("05918", "STAIR5.3WM", [32, 0, 0, 0, 0, 0, 0]),
    ("05918", "level1.3wm", [713_151, 1117, 1571, 715, 74, 1, 327]),
    ("05918", "level2.3wm", [618_323, 849, 1128, 454, 187, 1, 207]),
    ("05918", "level3.3wm", [493_026, 491, 683, 277, 89, 1, 124]),
    ("05918", "level3b.3wm", [58_987, 189, 447, 134, 25, 1, 29]),
    ("05918", "level4.3wm", [232_046, 426, 777, 360, 12, 1, 53]),
    ("05918", "stadium2.3wm", [417_740, 614, 1030, 460, 93, 1, 60]),
    ("05918", "stadium3.3wm", [887_606, 1187, 2013, 905, 148, 1, 133]),
    ("05918", "stair2.3wm", [159_548, 585, 1059, 485, 73, 1, 26]),
    ("05918", "stair3.3wm", [108_269, 331, 584, 260, 48, 1, 22]),
    ("05918", "stair4.3wm", [26_994, 84, 150, 55, 24, 1, 4]),
    ("05919", "abu.3wm", [140_035, 618, 861, 267, 319, 7, 25]),
    ("05920", "Floor1.3wm", [144_819, 166, 418, 86, 47, 2, 31]),
    ("05920", "Floor2.3wm", [124_300, 217, 475, 128, 23, 1, 65]),
    ("05920", "Floor3.3wm", [136_081, 242, 526, 153, 33, 5, 51]),
    ("05920", "Floor4.3wm", [61_904, 112, 277, 74, 23, 3, 12]),
    ("05920", "Floor5.3wm", [27_474, 75, 258, 46, 14, 2, 13]),
    ("05920", "Floor6.3wm", [29_705, 107, 227, 61, 32, 2, 12]),
    ("05920", "Floor7.3wm", [13_056, 38, 87, 19, 12, 2, 5]),
    ("05920", "Stair2.3wm", [2_711, 18, 18, 10, 6, 1, 1]),
    ("05920", "Stair3.3wm", [6_620, 49, 53, 33, 12, 2, 2]),
    ("05920", "Stair4.3wm", [17_145, 91, 145, 72, 12, 2, 5]),
    ("05920", "Stair5.3wm", [2_854, 17, 17, 9, 6, 1, 1]),
    ("05920", "Stair6.3wm", [3_592, 22, 22, 13, 7, 1, 1]),
    ("05920", "Stair7.3wm", [5_108, 39, 40, 29, 8, 1, 1]),
];

#[test]
fn walkmaps_self_check() {
    let Some(tours) = tours() else { return };
    let mut seen = 0;
    let (mut strings, mut empty, mut junk_after_last_semicolon, mut junk) = (0, 0, 0, 0);
    let (mut segments, mut empty_vis) = (0, 0);
    let mut totals = [0usize; 5];
    for (i, t) in tours.iter().enumerate() {
        let x = model(t);
        let counts = x.face_counts();
        let files: Vec<&str> = t.container.of_class(FileClass::WalkMap).collect();
        for f in &files {
            let (_, _, e) = WALKMAPS.iter().find(|(id, name, _)| *id == IDS[i] && name == f).unwrap_or_else(|| panic!("{f} not in table"));
            let bytes = read(t, f);
            let w = walkmap::parse(&bytes, &counts).unwrap();
            let got = [bytes.len(), w.cells.len(), w.polygon_count(), w.count(CellKind::Ground), w.count(CellKind::Hotspot), w.count(CellKind::Collision), w.count(CellKind::Visibility)];
            assert_eq!(&got, e, "{}/{f}", IDS[i]);
            seen += 1;
            totals[0] += w.cells.len();
            for (k, kind) in [CellKind::Ground, CellKind::Visibility, CellKind::Hotspot, CellKind::Collision].into_iter().enumerate() {
                totals[k + 1] += w.count(kind);
            }
            assert!(w.version == 3 || w.cells.is_empty());
            for c in &w.cells {
                assert!(c.polygons.iter().all(|p| p.len() == 3 || p.len() == 4));
                match &c.data {
                    CellData::Hotspot { enter, exit } => {
                        for h in [enter, exit] {
                            strings += 1;
                            let orig = h.original();
                            if orig.is_empty() {
                                empty += 1;
                            }
                            if !orig.is_empty() && !orig.ends_with(';') {
                                junk_after_last_semicolon += 1;
                            }
                            if h.junk_bytes > 0 {
                                junk += 1;
                            }
                        }
                    }
                    CellData::Collision { segments: s, .. } => {
                        segments += s.len();
                        assert!(s.iter().all(|s| s.top == 1.0 && s.climbable == 0));
                    }
                    CellData::Visibility(entries) => {
                        if c.polygons.is_empty() {
                            empty_vis += 1;
                        }
                        assert!(entries.windows(2).all(|p| p[0].mesh < p[1].mesh), "ascending mesh order");
                    }
                    CellData::Ground(p) => assert!(p.is_some()),
                }
            }
        }
    }
    assert_eq!(seen, WALKMAPS.len());
    assert_eq!(totals, [17_285, 12_052, 2_725, 2_425, 83]);
    assert_eq!((strings, empty, segments, empty_vis), (4_850, 816, 7_549, 3));
    // 701 non-empty strings do not end in ';' (4 of them are clean statements without a final
    // ';'); the printable-prefix rule finds 721 strings with junk (some junk holds ';' bytes).
    assert_eq!((junk_after_last_semicolon, junk), (701, 721));
    let start: Vec<_> = tours.iter().map(|t| t.start_walkmap.clone().unwrap()).collect();
    assert_eq!(start, ["acr.3wm", "1stFloor.3wm", "level1.3wm", "per.3wm", "level1.3wm", "abu.3wm", "Floor1.3wm"]);
    let loaded: Vec<_> = tours.iter().map(|t| t.walkmaps.len()).collect();
    assert_eq!(loaded, [1, 7, 9, 2, 10, 1, 13]);
    // §9 quick table: cells over all loaded maps.
    let cells: Vec<usize> = tours.iter().map(|t| t.summary().walkmap_cells).collect();
    assert_eq!(cells, [1_713, 1_069, 4_308, 2_511, 5_873, 618, 1_193]);
    // The worked example: acr.3wm cell 0 ground plane and cell 1273 collision.
    let acr = &tours[0].walkmaps[0].data;
    let CellData::Ground(Some(p)) = acr.cells[0].data else { panic!() };
    assert!((p[3] + 0.38102).abs() < 1e-4);
    let CellData::Collision { level, segments, .. } = &acr.cells[1273].data else { panic!() };
    assert_eq!((acr.cells[1273].polygons.len(), segments.len()), (43, 67));
    assert!((level - 0.21).abs() < 1e-4);
    let CellData::Visibility(v) = &acr.cells[1288].data else { panic!() };
    assert!(v.is_empty());
    assert!(matches!(acr.cells[1055].data, CellData::Hotspot { .. }));
}

#[test]
fn lights_self_check() {
    let Some(tours) = tours() else { return };
    // §7.2.2: version, nlights, Σcount, npaint, size.
    let expect = [
        ("acr.3lt", [3, 71, 132_071, 145, 1_115_320]),
        ("rom.3lt", [3, 105, 937_788, 0, 7_531_716]),
        ("bea.3lt", [3, 228, 523_424, 0, 4_251_244]),
        ("per.3lt", [3, 80, 689_154, 0, 5_535_644]),
        ("col.3lt", [3, 299, 881_205, 0, 7_133_372]),
        ("abu.3lt", [3, 128, 371_703, 0, 3_009_476]),
        ("Azuchi.3lt", [3, 203, 433_395, 3_291, 4_406_000]),
    ];
    for (t, (name, e)) in tours.iter().zip(expect) {
        let l = t.lights.as_ref().unwrap();
        assert_eq!(l.file, name);
        let size = read(t, name).len();
        assert_eq!([l.data.version as usize, l.data.lights.len(), l.data.entry_count(), l.data.paint.len(), size], e);
        // Every entry resolves to a mesh, face and slot (checked by the loader).
        assert!(!t.issues.iter().any(|i| matches!(i, Issue::BadLightEntry { .. } | Issue::BadPaint { .. })));
        assert!(l.data.lights.iter().all(|l| l.colour >> 24 == 0xFF));
    }
    let first = &tours[0].lights.as_ref().unwrap().data.lights[0];
    assert_eq!((first.name.as_str(), first.colour, first.face_refs.len()), ("_TORCH4", 0xFF53_1E01, 693));
    // Flicker statements that reach an existing light (§7.2.6).
    let expect = [(19, 19), (9, 9), (44, 44), (0, 53), (0, 7), (57, 59), (4, 7)];
    for (t, (hit, all)) in tours.iter().zip(expect) {
        let l = &t.lights.as_ref().unwrap().data;
        let flicker: Vec<_> = t.script_statements().into_iter().filter(|(_, s)| s.is_object_property(Subsystem::Lights, "FLICKER")).collect();
        let found = flicker.iter().filter(|(_, s)| l.by_script_name(s.object_name().unwrap()).is_some()).count();
        assert_eq!((found, flicker.len()), (hit, all));
    }
}

#[test]
fn clouds_viewpoints_layers() {
    let Some(tours) = tours() else { return };
    let acr = &tours[0];
    // §7.3.1: version, n, size, y range.
    let expect = [
        ("brzath.3cl", 3, 185_678, 2_970_860, (1, 367)),
        ("cartidm.3cl", 3, 86_971, 1_391_548, (1, 285)),
        ("carytid.3cl", 3, 86_931, 1_390_908, (1, 285)),
        ("chariot.3cl", 3, 124_867, 1_997_884, (1, 117)),
        ("nike.3cl", 3, 7_378, 118_060, (1, 82)),
        ("floral.3cl", 3, 6_300, 100_812, (0, 58)),
        ("wordcld.3cl", 2, 31_425, 502_808, (0, 162)),
    ];
    for (name, v, n, size, yr) in expect {
        let c = cloud::parse(&read(acr, name)).unwrap();
        assert_eq!((c.version, c.points.len(), read(acr, name).len(), c.y_range().unwrap()), (v, n, size, yr), "{name}");
        assert_eq!(c.trailer.is_some(), v == 3);
    }
    assert_eq!(acr.clouds.len(), 7);
    let mut placed: BTreeMap<&str, Vec<(String, f64)>> = BTreeMap::new();
    for p in &acr.cloud_placements {
        placed.entry(p.file.as_str()).or_default().push((p.mesh.clone(), p.scale));
        assert!(p.offset.is_some(), "{} has CLOUD.OFFSET", p.mesh);
    }
    assert_eq!(placed.len(), 5);
    assert_eq!(acr.cloud_placements.len(), 12);
    assert_eq!(placed["brzath.3cl"], vec![("STATUE_PLAN02".into(), 0.65)]);
    assert_eq!(placed["nike.3cl"].len(), 4);
    assert!(placed["cartidm.3cl"].iter().all(|(_, s)| *s == 0.88));
    for p in &acr.cloud_placements {
        assert!(model(acr).mesh_by_script_name(&p.mesh).is_some(), "placeholder {} exists", p.mesh);
    }

    // §7.7 .3vp.
    let vp = [(2, "level2.3vp", 7_685, 184_448), (4, "level3.3vp", 13_028, 312_680), (5, "abu.3vp", 2_863, 68_720)];
    for (i, name, n, size) in vp {
        let bytes = read(&tours[i], name);
        let v = viewpoints::parse(&bytes).unwrap();
        assert_eq!((v.points.len(), v.normals.len(), bytes.len()), (n, n, size));
        assert!(v.normals.iter().all(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-3 && n[1] > 0.0));
    }

    // §8.7 lh.32d: elements, _H, _L; and sizes.
    let expect = [(45, 26, 18, 17_386), (26, 16, 9, 10_104), (30, 14, 15, 11_631), (29, 19, 9, 11_250), (26, 13, 12, 10_093), (57, 34, 22, 21_994), (39, 16, 22, 15_037)];
    let begin = read(&tours[0], "begin.32d");
    for (t, (n, h, l, size)) in tours.iter().zip(expect) {
        let lh = layer2d::parse(&read(t, "lh.32d")).unwrap();
        assert_eq!(read(t, "lh.32d").len(), size);
        assert_eq!((lh.elements.len(), lh.elements.iter().filter(|e| e.is_hotspot_box()).count(), lh.elements.iter().filter(|e| e.is_caption()).count()), (n, h, l));
        assert!(lh.element("_dot").is_some());
        assert_eq!(lh.version, 4);
        for e in &lh.elements {
            let [l, tp, r, b] = e.rect();
            let wh = (r - l, b - tp);
            if e.is_caption() {
                assert_eq!(wh, (89.0, 22.0));
                assert!(e.colour_key && e.is_label && !e.captures_mouse);
            }
            if e.is_hotspot_box() {
                assert_eq!(wh, (91.0, 22.0));
                assert!(e.has_text && e.kind == layer2d::ElementKind::Hotspot);
            }
        }
        let b = read(t, "begin.32d");
        assert_eq!(b, begin, "begin.32d identical in every tour");
        let bl = layer2d::parse(&b).unwrap();
        assert_eq!(bl.elements.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["_1", "_0", "_2", "_3"]);
        assert_eq!(b.len(), 1548);
    }
}

#[test]
fn scripts_self_check() {
    let Some(tours) = tours() else { return };
    let mut statements = 0;
    let mut macros = 0;
    let mut runs = 0;
    let hotspots = [26, 14, 14, 18, 13, 18, 16];
    for (t, h) in tours.iter().zip(hotspots) {
        let s = t.summary();
        statements += s.script_statements;
        macros += s.macro_definitions;
        runs += s.run_commands;
        assert_eq!(s.hotspot_macros, h);
        assert!(t.container.of_class(FileClass::Script).all(|f| t.referenced.contains_key(f)), "every script is loaded from load.3sc");
        assert!(t.macros_run.contains("HIDEALL") && t.macros_run.len() == 1, "only HIDEALL runs");
        assert!(t.macros.contains_key("SHOWALL"));
        assert_eq!(t.alarms.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["LOADALARM", "USERLOCUPDATE"]);
        assert_eq!(t.alarms[0].1, "2D._0.NEXTFRAME(16");
        assert_eq!(t.alarms[1].1, "SCRIPT.EVENT(USERLOCUPDATE");
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].name, "WORLDLOADED");
        assert_eq!(t.events[0].get("fieldofview"), Some("80"));
        // Every lh.32d element is named by at least one script statement.
        let named: BTreeSet<String> = t.script_statements().iter().filter(|(_, s)| s.subsystem == Subsystem::Layer2d).filter_map(|(_, s)| s.object_name().map(str::to_string)).collect();
        for e in &t.final_layer().unwrap().data.elements {
            assert!(named.contains(&e.name.to_ascii_uppercase()), "{} unreferenced", e.name);
        }
        assert!(t.script_statements().iter().all(|(_, s)| s.subsystem != Subsystem::Unknown), "every script statement has a known subsystem");
    }
    assert_eq!((statements, macros, runs), (3_554, 15, 7));
    assert!(tours[4].macros.contains_key("LARGE"));
    // §4.1 WORLDLOADED values (XOFFSET, ZEXTENT, SM_ROTATEFRAME).
    let wl: Vec<_> = tours.iter().map(|t| (t.events[0].get("XOFFSET").unwrap().to_string(), t.events[0].get("SM_ROTATEFRAME").map(str::to_string))).collect();
    assert_eq!(wl[0].0, "-1.00");
    assert_eq!(wl[3], ("21.526201".to_string(), Some("-90".to_string())));
    assert_eq!(wl[6].1.as_deref(), Some("-90"));
    // The HIDEALL merge quirk: one glued statement sets WATER from "03D...".
    let acr = &tours[0];
    let glued: Vec<_> = acr.script_statements().into_iter().filter(|(_, s)| s.property == "WATER").collect();
    assert_eq!(glued.len(), 1);
    assert_eq!(glued[0].1.text, "3D.N_00WATER.WATER=03D.BOX23A.VISIBLE=02D._DOT.VISIBLE=0");
    assert_eq!(glued[0].1.flag(), Some(false));
    // Statement inventory spot checks (§3.8, after loading).
    let mut inv: BTreeMap<String, usize> = BTreeMap::new();
    for t in tours {
        for (k, v) in t.statement_inventory() {
            *inv.entry(k).or_default() += v;
        }
    }
    for (shape, n) in [
        ("2D.<id>.VISIBLE=", 680),
        ("3D.<id>.VISIBLE=", 299),
        ("3D.<id>.SPRITE=", 304),
        ("3D.<id>.FILMSTRIP=", 269),
        ("2D.<id>.ANCHORPOINT=", 254),
        ("2D.<id>.TEXT=", 224),
        ("LIGHTS.<id>.FLICKER=", 198),
        ("SCENE.DRAW", 55),
        ("SCENE.DRAW)", 7),
        ("SCENE.LOAD(", 29),
        ("WALKMAP.LOAD(", 43),
        ("SCENE.CURSOR=", 49),
        ("3D.<id>.WATER=", 1),
    ] {
        assert_eq!(inv.get(shape).copied().unwrap_or(0), n, "{shape}");
    }
    let hotspot_total: usize = tours.iter().map(|t| t.script_statements().iter().filter(|(_, s)| is_hotspot_click(s)).count()).sum();
    assert_eq!(hotspot_total, 119);
}

#[test]
fn name_cross_references() {
    let Some(tours) = tours() else { return };
    // §7.8 / §8.7: distinct script ids (all .3sc statements) missing from the .x, the .3lt, and lh.32d.
    let mesh_missing = [21, 2, 45, 4, 17, 9, 0];
    let light_missing = [0, 0, 0, 53, 7, 5, 3];
    let element_missing = [17, 13, 17, 28, 21, 0, 15];
    for (i, t) in tours.iter().enumerate() {
        let x = model(t);
        let l = &t.lights.as_ref().unwrap().data;
        let lh = &t.final_layer().unwrap().data;
        let (mut m, mut li, mut e) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for (_, s) in t.script_statements() {
            let Some(id) = s.object_name() else { continue };
            match s.subsystem {
                sub if sub.is_3d() && x.mesh_by_script_name(id).is_none() => {
                    m.insert(id.to_string());
                }
                Subsystem::Lights if l.by_script_name(id).is_none() => {
                    li.insert(id.to_string());
                }
                Subsystem::Layer2d if lh.element(id).is_none() && id != "_0" => {
                    e.insert(id.to_string());
                }
                _ => {}
            }
        }
        assert_eq!((m.len(), li.len(), e.len()), (mesh_missing[i], light_missing[i], element_missing[i]), "{} {m:?} {li:?} {e:?}", IDS[i]);
    }
    assert!(tours[0].issues.iter().any(|i| matches!(i, Issue::UnknownElement { id, .. } if id == "_L13")));
    assert!(tours[5].issues.iter().any(|i| matches!(i, Issue::UnknownLight { id, .. } if id == "_TORCH36")));
    // Hotspot strings in walk maps: every referenced walk map exists.
    for t in tours {
        assert!(!t.issues.iter().any(|i| matches!(i, Issue::MissingFile { kind, .. } if kind == "WalkMap")));
    }
}

#[test]
fn hotspot_string_statements() {
    let Some(tours) = tours() else { return };
    // Walk-map hotspot strings do more than show captions (spec §6.2): Acropolis switches the water
    // ripple on `n_00water` on and off, Roman Forum changes the eye height, Persepolis has a
    // lights-off cell and Abu Simbel activates an alarm that never exists.
    let from_cells = |t: &Tour| -> Vec<(String, String)> {
        t.deferred.iter().filter(|d| d.origin.contains(".3wm cell")).map(|d| (d.origin.clone(), d.statement.text.clone())).collect()
    };
    let acr = from_cells(&tours[0]);
    let water: Vec<_> = acr.iter().filter(|(_, s)| s.starts_with("3D.N_00WATER.WATER=")).collect();
    assert_eq!(water.len(), 8);
    assert!(water.iter().all(|(o, s)| s.ends_with(if o.ends_with("enter") { "=1" } else { "=0" })));
    assert!(model(&tours[0]).mesh_by_script_name("N_00WATER").is_some());
    let rom = from_cells(&tours[1]);
    assert_eq!(rom.iter().filter(|(_, s)| s.starts_with("CAMERA.HEIGHT=")).count(), 4);
    let per = from_cells(&tours[3]);
    assert!(per.iter().any(|(o, s)| o == "stair1.3wm cell 1255 enter" && s == "LIGHTS.OFF"));
    assert!(tours[5].issues.iter().any(|i| matches!(i, Issue::UnknownAlarm { name, .. } if name == "LITEANIM")));
    // Every walk map named by a hotspot string is one the scripts preload.
    for t in tours {
        let loaded: BTreeSet<String> = t.walkmaps.iter().map(|w| w.file.to_lowercase()).collect();
        for d in t.deferred.iter().filter(|d| d.origin.contains(".3wm cell")) {
            if let Some((_, f)) = d.statement.file_reference() {
                assert!(loaded.contains(&f.to_lowercase()), "{} {f}", d.origin);
            }
        }
    }
}
