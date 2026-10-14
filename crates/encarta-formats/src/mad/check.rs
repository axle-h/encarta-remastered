//! Whole-database validation: decode every tile and every grid record, verify the joins, and
//! compare with the counts and self-check values of mad.md §0.2, §4.5, §5.8, §7.x and §8.x.

use super::cells::CellSystem;
use super::grid::Grid;
use super::octahedral;
use super::rmap::{self, Raster, TILE_PIXELS, TileMode};
use super::schemas::*;
use super::tables::{self, GeometryKind};
use super::{Atlas, Edition, GRID_PAIRS, LCID_GRID_PAIRS, lcid_name};
use anyhow::Result;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

/// Outcome of a check run: named counters, a log, and the failed assertions.
#[derive(Debug, Default)]
pub struct Report {
    pub values: BTreeMap<String, u64>,
    pub log: Vec<String>,
    pub failures: Vec<String>,
}

impl Report {
    fn set(&mut self, name: &str, v: u64) {
        self.values.insert(name.to_string(), v);
    }

    /// Records a counter and checks it against the expected value, when there is one.
    fn count(&mut self, name: &str, got: u64, want: Option<u64>) {
        self.set(name, got);
        match want {
            Some(w) if w != got => self.fail(format!("{name}: {got}, expected {w}")),
            Some(_) => self.log.push(format!("ok   {name}: {got} (as specified)")),
            None => self.log.push(format!("ok   {name}: {got}")),
        }
    }

    fn assert(&mut self, cond: bool, what: impl Into<String>) {
        let what = what.into();
        if cond {
            self.log.push(format!("ok   {what}"));
        } else {
            self.fail(what);
        }
    }

    fn fail(&mut self, what: impl Into<String>) {
        let what = what.into();
        self.log.push(format!("FAIL {what}"));
        self.failures.push(what);
    }

    /// Runs a step; an error becomes a failure instead of aborting the run.
    fn step(&mut self, name: &str, f: impl FnOnce(&mut Report) -> Result<()>) {
        let t = Instant::now();
        if let Err(e) = f(self) {
            self.fail(format!("{name}: {e:#}"));
        }
        self.log.push(format!("     ({name}: {:.2} s)", t.elapsed().as_secs_f64()));
    }

    pub fn get(&self, name: &str) -> Option<u64> {
        self.values.get(name).copied()
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Spec numbers for one edition (`None` where the spec gives none).
struct Expected {
    tiles: Option<u64>,
    fill_tiles: Option<u64>,
    grid_records: &'static [(&'static str, u64)],
    geogem_records: Option<u64>,
    geogem_keys: Option<u64>,
    geogem_points: Option<u64>,
    geogem_refs: Option<u64>,
    entities: Option<u64>,
    find_entities: Option<u64>,
    center_entities: Option<u64>,
}

fn expected(edition: Option<Edition>) -> Expected {
    match edition {
        Some(Edition::World) => Expected {
            tiles: Some(357_792),
            fill_tiles: Some(188_702),
            grid_records: &[
                ("LegendGrid", 2),
                ("Bitmap", 149),
                ("EntityConv", 1_772),
                ("FindGECContextGrid0409", 1_356),
                ("FindGECContextGrid0009", 1),
                ("MillerTerrainGrid", 1_263),
                ("Find Grid0409", 1_356),
                ("Find Grid0009", 1),
                ("GeoObjGrid0409", 11_808),
                ("GeoObjGrid0009", 11_808),
                ("PDCAltitudeGrid", 33),
            ],
            geogem_records: Some(217_323),
            geogem_keys: Some(2_754_023),
            geogem_points: Some(54_448_380),
            geogem_refs: Some(2_947_064),
            entities: Some(1_813_920),
            find_entities: Some(1_798_103),
            center_entities: Some(2_015),
        },
        Some(Edition::Student) => Expected {
            tiles: Some(87_384),
            fill_tiles: Some(68_526),
            grid_records: &[("PDCAltitudeGrid", 33)],
            geogem_records: Some(20_491),
            geogem_keys: None,
            geogem_points: None,
            geogem_refs: Some(166_539),
            entities: None,
            find_entities: None,
            center_entities: None,
        },
        None => Expected {
            tiles: None,
            fill_tiles: None,
            grid_records: &[],
            geogem_records: None,
            geogem_keys: None,
            geogem_points: None,
            geogem_refs: None,
            entities: None,
            find_entities: None,
            center_entities: None,
        },
    }
}

/// §5.8: SHA-256 of tile buffers (WLD_CD).
const TILE_HASHES: &[(&str, u32, u32, u32, &str)] = &[
    ("TECTONIC", 0x0000_0000, 0, 4380, "f18f48d345416d040f1110d529de5878b4e93f491a4239b237f1a6fb109b3fb5"),
    ("TECTONIC", 0x1000_0005, 43492, 4048, "868a31aa11d279d8f4cd8c13e8fd38f92562ebb24de39b3b43eb6eee1c6c8e3b"),
    ("TECTONIC", 0x3000_00FF, 1346284, 240, "fe1b52c12cc9931501a40320a4792c93d38bf85330d0ebb561fe6537c3bd620d"),
    ("HYPSO", 0x0000_0000, 0, 3540, "30408780e8077c7937faa31d63ea307420843f2a45825552429623c2bbe51bb7"),
    ("HYPSO", 0x2000_0021, 256628, 6120, "8173b0e35c70d10b1baf4e147f2ae160d174e2372064683879a7ece02b3a5d95"),
    ("HYPSO", 0x8000_0000, 111233644, 1, "8eff7ce9c14a9d40b650cf195e74024d3d29418f98cad853a9b933b03fd5cb34"),
    ("SATELLITE", 0x5000_0000, 4343070, 544, "5a628b09152e73e2ec37dc8b1e4da896951c12257677cb5849a8d2c53d493208"),
    ("MOON", 0x0000_0003, 25900, 8560, "52184dcfe8eab648df6d12666f361a069bbcc820087d55389437e4609653fcd0"),
];

/// §7.8 GeoGem self-check (WLD_CD key 0x18).
const GEOGEM_HASH: &str = "8e4dd103d39773599588253d96a64bf0f9bb626a906136fbd84947d57a002cf3";

/// Runs every check. `edition` selects the spec numbers to compare with.
pub fn run(atlas: &Atlas, edition: Option<Edition>) -> Report {
    let mut rep = Report::default();
    let exp = expected(edition);
    let world = edition == Some(Edition::World);

    rep.step("indexes", check_indexes_of(atlas));
    rep.step("rasters", |rep| check_rasters(atlas, &exp, world, rep));
    let kinds = atlas.class_kinds().unwrap_or_default();
    let langs = atlas.languages().unwrap_or_else(|_| vec![0x409]);
    let mut geoobj: HashMap<u16, GeoObjIndex> = HashMap::new();
    rep.step("grid tables", |rep| check_grids(atlas, &exp, &langs, &mut geoobj, rep));
    rep.step("GeoGem", |rep| check_geogem(atlas, &exp, &kinds, geoobj.get(&0x409), world, rep));
    rep.step("gazetteer", |rep| check_gazetteer(atlas, &exp, geoobj.get(&0x409), world, rep));
    rep.step("plain tables", |rep| check_plain(atlas, world, rep));
    rep
}

fn check_indexes_of(atlas: &Atlas) -> impl FnOnce(&mut Report) -> Result<()> + '_ {
    move |rep| {
        let mut n = 0;
        for name in atlas.section_names() {
            let b = atlas.require(&name)?;
            if b.starts_with(b"INDX") {
                atlas.index(&name)?;
                n += 1;
            }
        }
        rep.count("INDX sections", n, None);
        let mut exact = 0;
        for (g, i) in grid_pairs(atlas) {
            let idx = atlas.index(&i)?;
            let len = atlas.require(&g)?.len() as u64;
            rep.assert(idx.data_end() == len, format!("{g}: last record ends at the section end ({} / {len})", idx.data_end()));
            exact += 1;
        }
        rep.set("grid indexes checked", exact);
        Ok(())
    }
}

/// Grid/index pairs present in this database.
fn grid_pairs(atlas: &Atlas) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = GRID_PAIRS.iter().map(|(g, i)| (g.to_string(), i.to_string())).collect();
    let langs = atlas.languages().unwrap_or_default();
    for (g, i) in LCID_GRID_PAIRS {
        for &l in &langs {
            v.push((lcid_name(g, l), lcid_name(i, l)));
        }
    }
    v.retain(|(g, i)| atlas.section(g).is_some() && atlas.section(i).is_some());
    v
}

#[derive(Default)]
struct TileStats {
    tiles: u64,
    fill: u64,
    rle: u64,
    rle_t: u64,
    raw: u64,
    max_pad: u32,
    bad_len: u64,
}

fn raster_stats(r: &Raster) -> Result<TileStats> {
    let records = r.index.records();
    let decoded: Vec<(u32, u32, TileMode, u32)> = records
        .par_iter()
        .map(|&(off, len, key)| {
            let t = r.decode_key(key)?;
            anyhow::ensure!(t.pixels.len() == TILE_PIXELS, "short tile");
            Ok((off, len, t.mode, t.padding_bits))
        })
        .collect::<Result<_>>()?;
    let by_record: HashMap<(u32, u32), (TileMode, u32)> = decoded.iter().map(|&(o, l, m, p)| ((o, l), (m, p))).collect();
    let mut s = TileStats::default();
    for run in r.index.runs() {
        let (mode, pad) = by_record[&(run.offset, run.length)];
        let n = run.count as u64;
        s.tiles += n;
        match mode {
            TileMode::Fill => s.fill += n,
            TileMode::Rle => s.rle += n,
            TileMode::RleTransposed => s.rle_t += n,
            TileMode::Raw => s.raw += n,
        }
        if mode != TileMode::Fill && run.length % 4 != 0 {
            s.bad_len += n;
        }
        s.max_pad = s.max_pad.max(pad);
    }
    Ok(s)
}

fn check_rasters(atlas: &Atlas, exp: &Expected, world: bool, rep: &mut Report) -> Result<()> {
    let terrain = atlas.terrain()?;
    let (mut tiles, mut fill) = (0, 0);
    for r in &terrain.rasters {
        let s = raster_stats(r)?;
        let full: u64 = (0..=r.max_level).map(|l| 4u64.pow(l + 1)).sum();
        // Every key of every level must be present.
        let keys_ok = r.index.runs().iter().all(|run| {
            (run.first..=run.last()).all(|k| {
                let (l, x, y) = rmap::tile_of_key(k);
                l <= r.max_level && x < rmap::tiles_per_side(l) && y < rmap::tiles_per_side(l) && rmap::tile_key(l, x, y) == k
            })
        });
        rep.assert(
            keys_ok && s.tiles == full && s.bad_len == 0 && s.max_pad <= 32,
            format!(
                "raster {}: {} tiles (levels 0…{}, {} fill, {} mode 2, {} mode 3, {} raw), padding ≤ {} bits",
                r.name, s.tiles, r.max_level, s.fill, s.rle_t, s.rle, s.raw, s.max_pad
            ),
        );
        tiles += s.tiles;
        fill += s.fill;
    }
    rep.count("RMAP tiles", tiles, exp.tiles);
    rep.count("RMAP fill tiles", fill, exp.fill_tiles);
    if world {
        for &(name, key, off, len, sha) in TILE_HASHES {
            let r = terrain.raster(name).ok_or_else(|| anyhow::anyhow!("no raster {name}"))?;
            let t = r.decode_key(key)?;
            let got = hex(&Sha256::digest(&t.pixels));
            rep.assert(
                r.index.lookup(key) == Some((off, len)) && got == sha,
                format!("tile hash {name} {key:#010x} ({off}, {len})"),
            );
        }
    }
    // TESS: every mesh vertex is a point of the projection grid at subdivision 16.
    let h = 16.0;
    let mut on_grid = 0;
    for v in &terrain.mesh_vertices {
        let (lat, lon) = octahedral::vector_lat_lon([v[0] as f64, v[1] as f64, v[2] as f64]);
        let (dx, dy) = octahedral::from_lat_lon(lat, lon, h);
        let w = octahedral::to_vector(dx.round(), dy.round(), h);
        let d = (w[0] - v[0] as f64).abs().max((w[1] - v[1] as f64).abs()).max((w[2] - v[2] as f64).abs());
        if d < 1e-4 {
            on_grid += 1;
        }
    }
    rep.assert(
        on_grid == terrain.mesh_vertices.len() && !terrain.mesh_vertices.is_empty(),
        format!("TESS: {on_grid} of {} mesh vertices reproduced by the projection", terrain.mesh_vertices.len()),
    );
    Ok(())
}

/// Entities of one GeoObj grid: per record key, the entity id of each entry.
type GeoObjIndex = HashMap<u32, Vec<u32>>;

fn decode_all<T: GridSchema + Send>(grid: &Grid) -> Result<Vec<(u32, T)>> {
    grid.index
        .records()
        .par_iter()
        .map(|&(off, len, key)| {
            let (v, _) = decode_bytes::<T>(grid, grid.bytes(off, len)?).map_err(|e| e.context(format!("{} key {key:#x}", grid.name)))?;
            Ok((key, v))
        })
        .collect()
}

fn check_grids(
    atlas: &Atlas,
    exp: &Expected,
    langs: &[u16],
    geoobj: &mut HashMap<u16, GeoObjIndex>,
    rep: &mut Report,
) -> Result<()> {
    let want = |name: &str| exp.grid_records.iter().find(|(n, _)| *n == name).map(|&(_, v)| v);
    for (g, i) in grid_pairs(atlas) {
        if g == "GeoGemGrid" {
            continue;
        }
        let grid = atlas.grid(&g, &i)?;
        let n = grid.index.records().len() as u64;
        let decoded = match g.as_str() {
            "LegendGrid" => decode_all::<Legend>(&grid).map(|v| v.len()),
            "Bitmap" => decode_all::<Bitmap>(&grid).map(|v| v.len()),
            "EntityConv" => decode_all::<EntityConvPage>(&grid).map(|v| v.len()),
            "PDCAltitudeGrid" => decode_all::<DisplayRules>(&grid).map(|v| v.len()),
            "MillerTerrainGrid" => decode_all::<MillerTile>(&grid).map(|v| v.len()),
            _ if g.starts_with("FindGECContextGrid") => decode_all::<FindContextPage>(&grid).map(|v| v.len()),
            _ if g.starts_with("Find Grid") => decode_all::<FindPage>(&grid).map(|v| v.len()),
            _ if g.starts_with("GeoObjGrid") => decode_all::<GeoObjRecord>(&grid).map(|v| {
                let lcid = u16::from_str_radix(&g[g.len() - 4..], 16).unwrap_or(0);
                let idx: GeoObjIndex = v.iter().map(|(k, r)| (*k, r.entries.iter().map(|e| e.entity).collect())).collect();
                let len = v.len();
                if langs.contains(&lcid) {
                    geoobj.insert(lcid, idx);
                }
                len
            }),
            _ => {
                rep.log.push(format!("     {g}: no schema, skipped"));
                continue;
            }
        };
        match decoded {
            Ok(d) => rep.count(&format!("{g} records decoded"), d as u64, want(&g).or(Some(n))),
            Err(e) => rep.fail(format!("{g}: {e:#}")),
        }
    }
    // The GeoObj grids of all languages hold the same entities in the same places.
    if let (Some(a), Some(b)) = (geoobj.get(&0x409), geoobj.get(&0x9)) {
        rep.assert(a == b, "GeoObjGrid0409 and GeoObjGrid0009 hold the same entity ids in the same entries");
    }
    Ok(())
}

fn check_geogem(
    atlas: &Atlas,
    exp: &Expected,
    kinds: &[GeometryKind],
    geoobj: Option<&GeoObjIndex>,
    world: bool,
    rep: &mut Report,
) -> Result<()> {
    let grid = atlas.geo_gem()?;
    let cells: CellSystem = atlas.cells()?;
    rep.count("GeoGem keys", grid.index.key_count(), exp.geogem_keys);
    let records = grid.index.records();
    #[derive(Default)]
    struct Acc {
        records: u64,
        points: u64,
        out_of_range: u64,
        refs: u64,
        unresolved: u64,
        parts: u64,
        bad_cell: u64,
    }
    let acc = records
        .par_iter()
        .map(|&(off, len, key)| -> Result<Acc> {
            let mut r = grid.decoder(grid.bytes(off, len)?);
            let g = GeoGemRecord::decode(&mut r, kinds).map_err(|e| e.context(format!("GeoGem key {key:#x}")))?;
            r.finish().map_err(|e| e.context(format!("GeoGem key {key:#x}")))?;
            let mut a = Acc { records: 1, points: g.point_count() as u64, ..Default::default() };
            a.out_of_range = g.x.iter().chain(&g.y).filter(|&&v| !(0..=1023).contains(&v)).count() as u64;
            if cells.cell(key).is_none() {
                a.bad_cell = 1;
            }
            for o in &g.objects {
                a.refs += 1;
                a.parts += o.parts.len() as u64;
                if let Some(obj) = geoobj {
                    let ok = obj.get(&o.object.key).is_some_and(|e| (o.object.entry as usize) < e.len());
                    if !ok {
                        a.unresolved += 1;
                    }
                }
            }
            Ok(a)
        })
        .try_reduce(Acc::default, |a, b| {
            Ok(Acc {
                records: a.records + b.records,
                points: a.points + b.points,
                out_of_range: a.out_of_range + b.out_of_range,
                refs: a.refs + b.refs,
                unresolved: a.unresolved + b.unresolved,
                parts: a.parts + b.parts,
                bad_cell: a.bad_cell + b.bad_cell,
            })
        })?;
    rep.count("GeoGem records decoded", acc.records, exp.geogem_records);
    rep.count("GeoGem points", acc.points, exp.geogem_points);
    rep.count("GeoGem parts", acc.parts, None);
    rep.assert(acc.out_of_range == 0, format!("GeoGem coordinates in 0…1023 ({} outside)", acc.out_of_range));
    rep.assert(acc.bad_cell == 0, format!("GeoGem keys are valid cells ({} not)", acc.bad_cell));
    rep.count("GeoGem object references", acc.refs, exp.geogem_refs);
    if geoobj.is_some() {
        rep.count("GeoGem references resolving into GeoObjGrid0409", acc.refs - acc.unresolved, Some(acc.refs));
    }
    if world {
        let mut r = grid.record(0x18)?;
        let g = GeoGemRecord::decode(&mut r, kinds)?;
        let mut h = Sha256::new();
        for (x, y) in g.x.iter().zip(&g.y) {
            h.update((*x as u16).to_le_bytes());
            h.update((*y as u16).to_le_bytes());
        }
        let parts: usize = g.objects.iter().map(|o| o.parts.len()).sum();
        let c = cells.cell(0x18).unwrap();
        let (lat, lon) = c.lat_lon(g.x[0] as f64, g.y[0] as f64);
        rep.assert(
            g.objects.len() == 627
                && parts == 6466
                && g.point_count() == 167_106
                && g.layout == 1
                && r.remaining_bits() == 4
                && hex(&h.finalize()) == GEOGEM_HASH
                && grid.index.lookup(0x18) == Some((1_382_675, 65_237))
                && (lat - 8.0938).abs() < 1e-4
                && lon.abs() < 1e-9,
            "GeoGem self-check key 0x18 (627 objects, 6466 parts, 167106 points, 4 bits left, hash, 8.0938° N 0°)",
        );
    }
    Ok(())
}

fn check_gazetteer(atlas: &Atlas, exp: &Expected, geoobj: Option<&GeoObjIndex>, world: bool, rep: &mut Report) -> Result<()> {
    let Some(geoobj) = geoobj else { anyhow::bail!("GeoObjGrid0409 was not decoded") };
    // GeoObj: each id exactly once.
    let mut place: HashMap<u32, (u32, u16)> = HashMap::new();
    let mut dup = 0;
    for (&k, ents) in geoobj {
        for (j, &e) in ents.iter().enumerate() {
            if place.insert(e, (k, j as u16)).is_some() {
                dup += 1;
            }
        }
    }
    rep.count("GeoObj distinct entities", place.len() as u64, exp.entities);
    rep.assert(dup == 0, format!("GeoObj: every entity id appears once ({dup} repeats)"));

    // EntityConv agrees with GeoObj, and EntityConvKey gives each page's first id.
    let conv = atlas.grid("EntityConv", "EntityConvLookup")?;
    let pages = decode_all::<EntityConvPage>(&conv)?;
    let keys = tables::key_table(atlas.require("EntityConvKey")?)?;
    let mut n = 0u64;
    let mut agree = 0u64;
    let mut firsts_ok = keys.len() == pages.len();
    let mut sorted = true;
    for (p, page) in &pages {
        if let Some(k) = keys.get(*p as usize) {
            firsts_ok &= page.entries.first().map(|e| e.entity) == k.value;
        }
        sorted &= page.entries.windows(2).all(|w| w[0].entity < w[1].entity);
        for e in &page.entries {
            n += 1;
            if place.get(&e.entity) == Some(&(e.geoobj_key, e.entry)) {
                agree += 1;
            }
        }
    }
    rep.count("EntityConv entries", n, exp.entities);
    rep.count("EntityConv entries matching GeoObj", agree, Some(place.len() as u64));
    rep.assert(firsts_ok && sorted, "EntityConvKey values are the first entity of each page; pages ascending");

    // Find: (type, ctx) consistent per entity, ids ⊂ GeoObj; FindGEC counts = Find Key values.
    if let Ok(find) = atlas.find_grid(0x409) {
        let pages = decode_all::<FindPage>(&find)?;
        let mut tc: HashMap<u32, (u16, u16)> = HashMap::new();
        let (mut inconsistent, mut missing, mut entries) = (0u64, 0u64, 0u64);
        for (_, p) in &pages {
            for e in &p.entries {
                entries += 1;
                match tc.insert(e.entity, (e.kind, e.context)) {
                    Some(prev) if prev != (e.kind, e.context) => inconsistent += 1,
                    None if !place.contains_key(&e.entity) => missing += 1,
                    _ => {}
                }
            }
        }
        rep.count("Find Grid0409 entries", entries, None);
        rep.count("Find Grid0409 distinct entities", tc.len() as u64, exp.find_entities);
        rep.assert(inconsistent == 0, format!("Find: one (type, context) per entity ({inconsistent} conflicts)"));
        rep.assert(missing == 0, format!("Find ids ⊂ GeoObj ids ({missing} missing)"));
        let fkey = tables::key_table(atlas.require("Find Key0409")?)?;
        let gec = atlas.grid("FindGECContextGrid0409", "FindGECContextLookup0409")?;
        let gpages = decode_all::<FindContextPage>(&gec)?;
        let matches = gpages.iter().filter(|(k, p)| fkey.get(*k as usize).and_then(|e| e.value) == Some(p.pairs.len() as u32)).count();
        rep.count("FindGECContext pages whose size equals the Find Key value", matches as u64, Some(fkey.len() as u64));
        let fd = atlas.find_data(0x409)?;
        let bad_type = pages.iter().flat_map(|(_, p)| &p.entries).filter(|e| e.kind as usize >= fd.types.len()).count();
        let bad_ctx = pages.iter().flat_map(|(_, p)| &p.entries).filter(|e| e.context as usize >= fd.contexts.len()).count();
        rep.assert(bad_type == 0 && bad_ctx == 0, "Find types and contexts index Find Data");
        if world {
            let e3: Vec<_> = pages.iter().flat_map(|(_, p)| &p.entries).filter(|e| e.entity == 3).collect();
            let g1 = atlas.g1g2()?;
            let keys: Vec<String> = e3.iter().map(|e| g1.decode(&e.key)).collect();
            rep.assert(
                e3.first().map(|e| (e.kind, e.context)) == Some((30, 236))
                    && fd.types.get(30).map(String::as_str) == Some("country")
                    && fd.contexts.get(236).map(String::as_str) == Some("Asia")
                    && keys.iter().any(|k| k == "Afghānestān [Afghanistan]"),
                "Find: entity 3 is a country in Asia with key \"Afghānestān [Afghanistan]\"",
            );
        }
    }

    // EntityCenter ids ⊂ GeoObj; centroids.
    let centers = atlas.entity_centers()?;
    let distinct: HashSet<u32> = centers.iter().map(|c| c.entity).collect();
    rep.count("EntityCenter distinct entities", distinct.len() as u64, exp.center_entities);
    rep.assert(distinct.iter().all(|e| place.contains_key(e)), "EntityCenter ids ⊂ GeoObj ids");
    if world {
        let cells = atlas.cells()?;
        let at = |id: u32| {
            centers.iter().find(|c| c.entity == id).and_then(|c| cells.cell(c.cell_key).map(|cell| cell.lat_lon(c.x as f64, c.y as f64)))
        };
        let near = |p: Option<(f64, f64)>, lat: f64, lon: f64| p.is_some_and(|(a, b)| (a - lat).abs() < 1e-3 && (b - lon).abs() < 1e-3);
        rep.assert(near(at(1), 12.498, -69.937), format!("EntityCenter: entity 1 (Aruba) at 12.498 N 69.937 W: {:?}", at(1)));
        rep.assert(near(at(2), 17.146, -61.821), "EntityCenter: entity 2 at 17.146 N 61.821 W");
        rep.assert(near(at(3), 33.833, 66.025), "EntityCenter: entity 3 at 33.833 N 66.025 E");
        let gz = super::gazetteer::Gazetteer::open(atlas, 0x409)?;
        let loc = gz.locate(1)?;
        let name = loc.map(|l| gz.geo_obj_entry(l.geoobj_key, l.entry)).transpose()?.map(|e| e.name);
        rep.assert(
            loc.map(|l| (l.geoobj_key, l.entry)) == Some((2, 108)) && name.as_deref() == Some("Aruba (Neth.)"),
            format!("entity 1 = GeoObj key 2 entry 108 {name:?}"),
        );
    }
    Ok(())
}

fn check_plain(atlas: &Atlas, world: bool, rep: &mut Report) -> Result<()> {
    let classes = atlas.classes()?;
    rep.count("PDCAttribute rows", classes.len() as u64, if world { Some(606) } else { None });
    let cells = atlas.cells()?;
    let firsts: Vec<u32> = cells.bands.iter().map(|b| b.first_key).collect();
    rep.assert(firsts == [0, 8, 16, 24, 56, 184, 696, 2744, 35512, 559800], format!("GeoFaceScale first keys {firsts:?}"));
    let g = atlas.g1g2()?;
    rep.assert(
        g.decode(&[0x91, 0x41, 0x62, 0x62, 0x03, 0xAA, 0x02, 0x73]) == "‘Abbās",
        "G1G2: 91 41 62 62 03 AA 02 73 → ‘Abbās",
    );
    for lcid in atlas.languages()? {
        let name = lcid_name("Find Data", lcid);
        if atlas.section(&name).is_some() {
            let fd = atlas.find_data(lcid)?;
            rep.log.push(format!("ok   {name}: {} types, {} contexts, parses to its exact length", fd.types.len(), fd.contexts.len()));
        }
        let name = lcid_name("Find Key", lcid);
        if let Some(b) = atlas.section(&name) {
            rep.log.push(format!("ok   {name}: {} entries", tables::key_table(b)?.len()));
        }
    }
    let styles = atlas.map_styles()?;
    rep.assert(styles.first().map(|s| s.name.as_str()) == Some("Atlas Default"), format!("MapStyle: {} styles", styles.len()));
    let pens = tables::pen_defs(atlas.require("GeoPenDef")?)?;
    let labels = tables::label_styles(atlas.require("GeoLabel")?)?;
    let fonts = tables::label_fonts(atlas.require("Label Font")?)?;
    let shields = tables::shield_ids(atlas.require("ShieldId")?)?;
    let palette = atlas.bitmap_palette()?;
    let colors = tables::bitmap_color(atlas.require("BitmapColor")?)?;
    rep.log.push(format!(
        "ok   GeoPenDef {} pens, GeoLabel {}, Label Font {}, ShieldId {}, BitmapPalette {}, BitmapColor {}",
        pens.len(),
        labels.len(),
        fonts.len(),
        shields.len(),
        palette.len(),
        colors.len()
    ));
    rep.assert(palette.first().is_some_and(|p| p.rgb == [0, 255, 0] && p.transparent), "BitmapPalette index 0 is the transparent key colour");
    // BitmapDimDir matches the Bitmap records.
    let dims = tables::bitmap_dims(atlas.require("BitmapDimDir")?)?;
    let bm = atlas.grid("Bitmap", "BitmapLookup")?;
    let bitmaps = decode_all::<Bitmap>(&bm)?;
    let mut sorted: Vec<_> = bitmaps.iter().map(|(k, b)| (*k, (b.width as u8, b.height as u8))).collect();
    sorted.sort();
    rep.assert(
        sorted.len() == dims.len() && sorted.iter().zip(&dims).all(|(a, b)| a.1 == *b),
        format!("BitmapDimDir matches all {} Bitmap sizes", dims.len()),
    );
    if let Some(b) = atlas.section("MillerStyles") {
        let m = tables::miller_styles(b)?;
        let names: Vec<&str> = m.entries.iter().map(|e| e.raster.as_str()).collect();
        rep.log.push(format!("ok   MillerStyles: {} levels, rasters {}", m.levels.len(), names.join(" ")));
        if world {
            rep.assert(m.levels.first() == Some(&(512, 384)) && m.entries.len() == 11, "MillerStyles: level 0 is 512 × 384, 11 rasters");
        }
    }
    let ramps = tables::geo_trans(atlas.require("GeoTrans")?)?;
    let rects = tables::min_rects(atlas.require("MinRect")?)?;
    let attrs = tables::unicode_attributes(atlas.require("Unicode Attributes")?)?;
    let conv = tables::font_conv(atlas.require("Font Conv")?)?;
    let max_tint = pens.iter().map(|p| p.b8[3]).filter(|&t| t != 0xFF).max().unwrap_or(0) as usize;
    rep.assert(max_tint < ramps.len(), format!("GeoTrans: {} ramps cover every pen tint (max {max_tint})", ramps.len()));
    rep.assert(rects.iter().all(|r| styles.iter().any(|s| s.id == r.style)), format!("MinRect: {} rows, all for known styles", rects.len()));
    rep.assert(styles.iter().filter(|s| s.is_default).count() == 1, "MapStyle: exactly one default style");
    rep.log.push(format!("ok   Unicode Attributes {} records, Font Conv {} code pages", attrs.len(), conv.len()));
    if world {
        rep.assert(classes.get(41).map(|c| c.name.as_str()) == Some("River"), "PDCAttribute row 41 is River");
        let leg = atlas.grid("LegendGrid", "LegendLookup")?;
        let l: Legend = decode_record(&leg, 0x409)?;
        let texts: Vec<&str> = l.entries.iter().map(|e| e.text.as_str()).collect();
        rep.assert(texts == ["1,000,000+ people", "Populated Places", "National Capital"], "LegendGrid 0x409 entries");
        let b0 = bitmaps.iter().find(|(k, _)| *k == 0x10000).map(|(_, b)| b);
        rep.assert(
            b0.is_some_and(|b| b.width == 6 && b.height == 6 && b.pixels[..12] == [1, 2, 2, 2, 2, 2, 1, 3, 3, 3, 3, 2]),
            "Bitmap 0x10000 is 6×6 starting 1 2 2 2 2 2 1 3 3 3 3 2",
        );
        let alt = atlas.grid("PDCAltitudeGrid", "PDCAltitudeLookup")?;
        let d: DisplayRules = decode_record(&alt, 0)?;
        let r0 = &d.rules[..3];
        rep.assert(
            d.rules.len() == 1254
                && r0.iter().all(|r| r.class == 0)
                && r0.iter().map(|r| (r.max_scale, r.min_scale)).eq([(50_000_000, 4_000_000), (3_999_999, 150_000), (149_999, 1_000)])
                && r0.iter().map(|r| r.w5_13[0]).eq([114, 106, 105]),
            "PDCAltitudeGrid style 0: 1254 rules, class 0 scale ranges and w5",
        );
        let mt = atlas.grid("MillerTerrainGrid", "MillerTerrainLku")?;
        let t0: MillerTile = decode_record(&mt, 0)?;
        let t2: MillerTile = decode_record(&mt, 2)?;
        rep.assert(
            (t0.width, t0.height, t0.slot, t2.width, t2.height, t2.slot) == (256, 255, 0, 256, 129, 2),
            "MillerTerrainGrid keys 0 and 2 sizes",
        );
        let go = atlas.geo_obj(0x409)?;
        let r: GeoObjRecord = decode_record(&go, 0)?;
        let m = r.entries.iter().filter(|e| e.label.is_some()).count();
        let first: Vec<(u32, u8, u32, &str)> = r.entries.iter().take(5).map(|e| (e.entity, e.class, e.scale, e.name.as_str())).collect();
        rep.assert(
            r.entries.len() == 389
                && m == 378
                && first[0] == (35473, 0, 995, "Volta")
                && first[4].3 == "Danube\u{4}Donau"
                && r.entries[0].label == Some((1224, 81, 329)),
            "GeoObjGrid0409 key 0: 389 entries, 378 labelled, Volta … Danube/Donau",
        );
    }
    Ok(())
}
