//! Gazetteer → `out/atlas/gazetteer.sqlite`: one row per atlas entity (GeoObj), with feature type
//! and administrative context (Find Grid / Find Data), names and search keys (FTS5), position and
//! extent (GeoObj "go to" point, EntityCenter, GeoGem geometry), P2MTBL links, SDB country ids and
//! the Geography Quiz pools. Schema and ranking: docs/atlas-output.md.

use super::objects::{Classes, Objects};
use anyhow::{Context, Result};
use encarta_formats::mad::Atlas;
use encarta_formats::mad::schemas::{FindPage, GeoGemRecord, decode_record};
use encarta_formats::mad::tables::GeometryKind;
use encarta_formats::{msworld, p2m};
use rayon::prelude::*;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Geometry facts of one GeoObj object, over every band and cell.
#[derive(Debug, Clone)]
pub struct Extent {
    /// `(west, south, east, north)` with longitudes in −180…180, and the same in 0…360.
    b180: [f64; 4],
    b360: [f64; 4],
    pub classes: BTreeSet<u16>,
    pub min_band: u8,
    pub max_band: u8,
    /// First point of a point-class part in the finest band seen.
    pub point: Option<(f64, f64, u8)>,
}

impl Default for Extent {
    fn default() -> Self {
        Extent {
            b180: [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
            b360: [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
            classes: BTreeSet::new(),
            min_band: u8::MAX,
            max_band: 0,
            point: None,
        }
    }
}

impl Extent {
    fn add(&mut self, lat: f64, lon: f64) {
        let l360 = if lon < 0.0 { lon + 360.0 } else { lon };
        for (b, x) in [(&mut self.b180, lon), (&mut self.b360, l360)] {
            b[0] = b[0].min(x);
            b[1] = b[1].min(lat);
            b[2] = b[2].max(x);
            b[3] = b[3].max(lat);
        }
    }

    fn merge(&mut self, o: &Extent) {
        for (a, b) in [(&mut self.b180, &o.b180), (&mut self.b360, &o.b360)] {
            a[0] = a[0].min(b[0]);
            a[1] = a[1].min(b[1]);
            a[2] = a[2].max(b[2]);
            a[3] = a[3].max(b[3]);
        }
        self.classes.extend(o.classes.iter().copied());
        self.min_band = self.min_band.min(o.min_band);
        self.max_band = self.max_band.max(o.max_band);
        if let Some(p) = o.point
            && self.point.is_none_or(|q| p.2 > q.2)
        {
            self.point = Some(p);
        }
    }

    /// The narrower of the two bounding boxes; `west > east` means it crosses the antimeridian.
    pub fn bbox(&self) -> Option<[f64; 4]> {
        if self.b180[0] > self.b180[2] {
            return None;
        }
        let w180 = self.b180[2] - self.b180[0];
        let w360 = self.b360[2] - self.b360[0];
        if w360 + 1e-9 < w180 {
            let f = |x: f64| if x > 180.0 { x - 360.0 } else { x };
            Some([f(self.b360[0]), self.b360[1], f(self.b360[2]), self.b360[3]])
        } else {
            Some(self.b180)
        }
    }

    pub fn centre(&self) -> Option<(f64, f64)> {
        let b = self.bbox()?;
        let mut lon = (b[0] + b[2]) / 2.0;
        if b[0] > b[2] {
            lon = (b[0] + b[2] + 360.0) / 2.0;
            if lon > 180.0 {
                lon -= 360.0;
            }
        }
        Some(((b[1] + b[3]) / 2.0, lon))
    }
}

/// Extents of every object (index = [`Objects`] index), from all GeoGem bands.
pub fn extents(atlas: &Atlas, objects: &Objects, classes: &Classes) -> Result<Vec<Extent>> {
    let gem = atlas.geo_gem()?;
    let cells = atlas.cells()?;
    let runs = gem.index.runs().to_vec();
    let partial: Vec<Vec<(u32, Extent)>> = runs
        .par_iter()
        .map(|run| -> Result<Vec<(u32, Extent)>> {
            let rec = GeoGemRecord::decode(&mut gem.decoder(gem.bytes(run.offset, run.length)?), &classes.kinds)?;
            let mut m: HashMap<u32, Extent> = HashMap::new();
            for key in run.first..=run.last() {
                let Some(cell) = cells.cell(key) else { continue };
                for o in &rec.objects {
                    let Some(i) = objects.index(o.object) else { continue };
                    let e = m.entry(i).or_default();
                    e.min_band = e.min_band.min(cell.band as u8);
                    e.max_band = e.max_band.max(cell.band as u8);
                    for p in &o.parts {
                        e.classes.insert(p.class);
                        for (j, (x, y)) in rec.points(p).enumerate() {
                            let (lat, lon) = cell.lat_lon(x as f64, y as f64);
                            e.add(lat, lon);
                            if j == 0
                                && classes.kinds[p.class as usize] == GeometryKind::Point
                                && e.point.is_none_or(|q| cell.band as u8 > q.2)
                            {
                                e.point = Some((lat, lon, cell.band as u8));
                            }
                        }
                    }
                }
            }
            Ok(m.into_iter().collect())
        })
        .collect::<Result<_>>()?;
    let mut out = vec![Extent::default(); objects.objs.len()];
    for part in partial {
        for (i, e) in part {
            out[i as usize].merge(&e);
        }
    }
    Ok(out)
}

#[derive(Debug, Default, Serialize)]
pub struct GazetteerReport {
    pub path: String,
    pub places: u64,
    pub names: u64,
    pub search_keys: u64,
    pub with_find_type: u64,
    pub with_geometry: u64,
    pub position_sources: HashMap<String, u64>,
    pub links: u64,
    pub linked_places: u64,
    pub link_places_not_in_atlas: u64,
    pub sdb_countries: u64,
    pub sdb_countries_not_in_atlas: Vec<u32>,
    pub quiz_rows: u64,
    pub bytes: u64,
    pub seconds: f64,
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path())
}

/// Search keys use 0x08/0x09 between rotated words and a bracketed canonical name.
fn clean_key(k: &str) -> Vec<String> {
    let k: String = k.chars().map(|c| if c == '\u{8}' || c == '\u{9}' { ' ' } else { c }).collect();
    let mut out = Vec::new();
    match k.split_once('[') {
        Some((a, b)) => {
            out.push(a.trim().to_string());
            out.push(b.trim_end_matches(']').trim().to_string());
        }
        None => out.push(k.trim().to_string()),
    }
    out.retain(|s| !s.is_empty());
    out.iter_mut().for_each(|s| *s = s.split_whitespace().collect::<Vec<_>>().join(" "));
    out
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = OFF;
PRAGMA synchronous = OFF;
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE class (row INTEGER PRIMARY KEY, id INTEGER, name TEXT, kind TEXT, z_order INTEGER, priority INTEGER, goto_km REAL);
CREATE TABLE feature_type (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE context (id INTEGER PRIMARY KEY, text TEXT);
CREATE TABLE place (
    entity INTEGER PRIMARY KEY,
    name TEXT,
    abbr TEXT,
    name_0009 TEXT,
    anonymous INTEGER NOT NULL,
    body TEXT NOT NULL,
    feature_type_id INTEGER,
    context_id INTEGER,
    class INTEGER, classes TEXT,
    lat REAL, lon REAL, position_source TEXT,
    west REAL, south REAL, east REAL, north REAL,
    goto_km REAL,
    min_band INTEGER, max_band INTEGER,
    geoobj_key INTEGER, geoobj_entry INTEGER,
    sdb_country INTEGER NOT NULL,
    link_count INTEGER NOT NULL,
    importance REAL NOT NULL
);
CREATE TABLE place_name (id INTEGER PRIMARY KEY, entity INTEGER NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL);
CREATE VIRTUAL TABLE place_fts USING fts5(name, entity UNINDEXED, kind UNINDEXED, content = 'place_name', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3');
CREATE VIEW place_full AS
    SELECT p.*, t.name AS feature_type, c.text AS context, k.name AS class_name
    FROM place p LEFT JOIN feature_type t ON t.id = p.feature_type_id LEFT JOIN context c ON c.id = p.context_id
    LEFT JOIN class k ON k.row = p.class;
CREATE TABLE place_link (entity INTEGER NOT NULL, kind INTEGER NOT NULL, kind_label TEXT NOT NULL, content_id INTEGER NOT NULL);
CREATE TABLE sdb_country (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE quiz_place (level INTEGER, kind INTEGER, region INTEGER, entity INTEGER);
"#;

pub fn run(atlas: &Atlas, objects: &Objects, classes: &Classes, extracted: &Path, out: &Path, log: &dyn Fn(String)) -> Result<GazetteerReport> {
    let t0 = std::time::Instant::now();
    let mut rep = GazetteerReport { path: "gazetteer.sqlite".into(), ..Default::default() };
    let cells = atlas.cells()?;
    let ext = extents(atlas, objects, classes)?;
    log(format!("  geometry extents of {} objects", ext.iter().filter(|e| e.min_band != u8::MAX).count()));

    // Find Grid: type, context and search keys per entity (all languages).
    let g1g2 = atlas.g1g2()?;
    let mut find: HashMap<u32, (u16, u16, Vec<String>)> = HashMap::new();
    let mut find_data = None;
    for &lcid in &objects.lcids {
        let Ok(grid) = atlas.find_grid(lcid) else { continue };
        if find_data.is_none() || lcid == 0x0409 {
            find_data = Some(atlas.find_data(lcid)?);
        }
        let pages: Vec<u32> = grid.index.runs().iter().flat_map(|r| r.first..=r.last()).collect();
        let decoded: Vec<Vec<(u32, u16, u16, String)>> = pages
            .par_iter()
            .map(|&p| -> Result<_> {
                let page: FindPage = decode_record(&grid, p)?;
                Ok(page.entries.into_iter().map(|e| (e.entity, e.kind, e.context, g1g2.decode(&e.key))).collect())
            })
            .collect::<Result<_>>()?;
        for (id, t, c, key) in decoded.into_iter().flatten() {
            let e = find.entry(id).or_insert((t, c, Vec::new()));
            if lcid == 0x0409 {
                (e.0, e.1) = (t, c);
            }
            e.2.push(key);
        }
    }
    let find_data = find_data.context("no Find Data")?;
    let mut centers: HashMap<u32, (f64, f64)> = HashMap::new();
    for c in atlas.entity_centers()? {
        if let Some(cell) = cells.cell(c.cell_key) {
            centers.insert(c.entity, cell.lat_lon(c.x as f64, c.y as f64));
        }
    }

    // Side files: P2MTBL links, SDB countries, quiz pools.
    let enc = extracted.join("iso/EE/ENCARTA");
    let links = match find_ci(&enc, "P2MTBL.DAT") {
        Some(p) => p2m::parse(&std::fs::read(&p)?)?.places,
        None => Vec::new(),
    };
    let sdb = match find_ci(&enc, "MSWORLD.SDB") {
        Some(p) => Some(msworld::parse_sdb(&std::fs::read(&p)?)?),
        None => None,
    };
    let sdb_ids: HashMap<u32, String> = sdb.iter().flat_map(|s| s.countries.iter().map(|c| (c.id, c.name.clone()))).collect();
    let mut link_count: HashMap<u32, (u32, bool)> = HashMap::new();
    for p in &links {
        let e = link_count.entry(p.place_id).or_default();
        e.0 += p.links.len() as u32;
        e.1 |= p.links.iter().any(|l| l.kind == 8);
    }

    let path = out.join("gazetteer.sqlite");
    let tmp = out.join("gazetteer.sqlite.tmp");
    std::fs::remove_file(&tmp).ok();
    let mut db = Connection::open(&tmp)?;
    db.execute_batch(SCHEMA)?;
    let tx = db.transaction()?;
    {
        let mut meta = tx.prepare("INSERT INTO meta VALUES (?1, ?2)")?;
        meta.execute(params!["source", "WLD_HD.MAD + WLD_CD.MAD, P2MTBL.DAT, MSWORLD.SDB, D1-D4PLACES.TXT"])?;
        meta.execute(params!["lcids", format!("{:04x?}", objects.lcids)])?;
        meta.execute(params!["search_sql", SEARCH_SQL])?;
        let mut st = tx.prepare("INSERT INTO class VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
        for (i, c) in classes.classes.iter().enumerate() {
            st.execute(params![i as i64, c.id, c.name, format!("{:?}", c.kind).to_lowercase(), c.group, c.priority, c.scale as f64 / 1000.0])?;
        }
        let mut st = tx.prepare("INSERT INTO feature_type VALUES (?1, ?2)")?;
        for (i, t) in find_data.types.iter().enumerate() {
            st.execute(params![i as i64, t])?;
        }
        let mut st = tx.prepare("INSERT INTO context VALUES (?1, ?2)")?;
        for (i, t) in find_data.contexts.iter().enumerate() {
            st.execute(params![i as i64, t])?;
        }
        let mut place = tx.prepare(
            "INSERT INTO place VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
        )?;
        let mut name_st = tx.prepare("INSERT INTO place_name (entity, name, kind) VALUES (?1, ?2, ?3)")?;
        let mut keys_of: HashMap<u32, u32> = HashMap::new();
        for (i, o) in objects.objs.iter().enumerate() {
            let e = &ext[i];
            let has_geom = e.min_band != u8::MAX;
            let f = find.get(&o.entity);
            // Main class: the most important (lowest priority value) class among its parts.
            let main = e.classes.iter().copied().min_by_key(|&c| (classes.classes[c as usize].priority, c));
            let body = if main.is_some_and(|c| classes.is_moon(c as usize)) { "moon" } else { "earth" };
            let (pos, src) = if let Some(p) = o.goto_point(&cells) {
                (Some(p), "goto")
            } else if let Some(&p) = centers.get(&o.entity) {
                (Some(p), "entity_center")
            } else if let Some((lat, lon, _)) = e.point {
                (Some((lat, lon)), "point")
            } else if let Some(c) = e.centre() {
                (Some(c), "geometry_bbox")
            } else {
                (None, "none")
            };
            *rep.position_sources.entry(src.to_string()).or_default() += 1;
            let bbox = e.bbox();
            let (links_n, has_article) = link_count.get(&o.entity).copied().unwrap_or_default();
            let is_country = sdb_ids.contains_key(&o.entity);
            let prio = main.map_or(255, |c| classes.classes[c as usize].priority) as f64;
            let goto_km = if o.goto_km > 0 { o.goto_km as f64 } else { main.map_or(0.0, |c| classes.classes[c as usize].scale as f64 / 1000.0) };
            let importance = if o.name.anonymous { -10.0 } else { 0.0 }
                + if is_country { 4.0 } else { 0.0 }
                + if has_article { 2.0 } else { 0.0 }
                + (links_n.min(10) as f64) / 10.0
                + (1.0 + goto_km).log10()
                + (255.0 - prio) / 127.5
                + if f.is_some() { 0.5 } else { 0.0 };
            let loc = objects.location(i as u32);
            place.execute(params![
                o.entity,
                (!o.name.primary.is_empty()).then_some(&o.name.primary),
                o.name.abbr,
                o.name_alt_lcid.as_ref().map(|n| super::objects::split_name(n).primary),
                o.name.anonymous,
                body,
                f.map(|f| f.0),
                f.map(|f| f.1),
                main,
                has_geom.then(|| serde_json::to_string(&e.classes).unwrap()),
                pos.map(|p| p.0),
                pos.map(|p| p.1),
                src,
                bbox.map(|b| b[0]),
                bbox.map(|b| b[1]),
                bbox.map(|b| b[2]),
                bbox.map(|b| b[3]),
                goto_km,
                has_geom.then_some(e.min_band),
                has_geom.then_some(e.max_band),
                loc.0,
                loc.1,
                is_country,
                links_n,
                importance,
            ])?;
            rep.places += 1;
            if has_geom {
                rep.with_geometry += 1;
            }
            if f.is_some() {
                rep.with_find_type += 1;
            }
            // Names: display, abbreviation, alternatives, other language, search keys.
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut add = |n: &str, kind: &str, rep: &mut GazetteerReport| -> Result<()> {
                let n = n.trim();
                if n.is_empty() || !seen.insert(n.to_lowercase()) {
                    return Ok(());
                }
                name_st.execute(params![o.entity, n, kind])?;
                if kind == "search_key" {
                    rep.search_keys += 1;
                } else {
                    rep.names += 1;
                }
                Ok(())
            };
            if !o.name.anonymous {
                add(&o.name.primary, "display", &mut rep)?;
                if let Some(a) = &o.name.abbr {
                    add(a, "abbr", &mut rep)?;
                }
                for a in &o.name.alternatives {
                    add(a, "alternative", &mut rep)?;
                }
                if let Some(n) = &o.name_alt_lcid {
                    for part in n.split('\u{4}') {
                        add(part.split('\u{2}').next().unwrap_or(part), "lcid_0009", &mut rep)?;
                    }
                }
                if let Some(f) = f {
                    for k in &f.2 {
                        for s in clean_key(k) {
                            add(&s, "search_key", &mut rep)?;
                        }
                    }
                }
            }
            *keys_of.entry(o.entity).or_default() += 1;
        }
        let mut st = tx.prepare("INSERT INTO place_link VALUES (?1, ?2, ?3, ?4)")?;
        for p in &links {
            if !keys_of.contains_key(&p.place_id) {
                rep.link_places_not_in_atlas += 1;
            }
            rep.linked_places += 1;
            for l in &p.links {
                st.execute(params![p.place_id, l.kind, p2m::kind_label(l.kind), l.content_id])?;
                rep.links += 1;
            }
        }
        let mut st = tx.prepare("INSERT INTO sdb_country VALUES (?1, ?2)")?;
        for (id, name) in &sdb_ids {
            st.execute(params![id, name])?;
            rep.sdb_countries += 1;
            if !keys_of.contains_key(id) {
                rep.sdb_countries_not_in_atlas.push(*id);
            }
        }
        let mut st = tx.prepare("INSERT INTO quiz_place VALUES (?1, ?2, ?3, ?4)")?;
        for level in 1..=4 {
            let Some(p) = find_ci(&enc, &format!("D{level}PLACES.TXT")) else { continue };
            for q in msworld::parse_quiz_places(&std::fs::read(&p)?)? {
                st.execute(params![level, q.kind, q.group, q.entity])?;
                rep.quiz_rows += 1;
            }
        }
    }
    tx.commit()?;
    db.execute_batch(
        "CREATE INDEX place_name_entity ON place_name(entity);
         CREATE INDEX place_link_entity ON place_link(entity);
         CREATE INDEX place_link_content ON place_link(content_id);
         INSERT INTO place_fts(place_fts) VALUES ('rebuild');
         INSERT INTO place_fts(place_fts) VALUES ('optimize');",
    )?;
    drop(db);
    std::fs::rename(&tmp, &path)?;
    rep.bytes = std::fs::metadata(&path)?.len();
    rep.seconds = t0.elapsed().as_secs_f64();
    log(format!("  gazetteer.sqlite: {} places, {} names, {} search keys, {:.0} MB", rep.places, rep.names, rep.search_keys, rep.bytes as f64 / 1e6));
    Ok(rep)
}

/// The place-search query the server should use (documented in docs/atlas-output.md).
pub const SEARCH_SQL: &str = "WITH hits AS (
    SELECT entity, max(lower(name) = lower(?2)) AS exact, min(rank) AS r FROM place_fts WHERE place_fts MATCH ?1 GROUP BY entity)
  SELECT p.entity, p.name, p.feature_type, p.context, p.lat, p.lon
  FROM hits h JOIN place_full p ON p.entity = h.entity
  ORDER BY h.exact DESC, p.importance DESC, h.r
  LIMIT 20";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_key_cleaning() {
        assert_eq!(clean_key("Afghānestān [Afghanistan]"), vec!["Afghānestān", "Afghanistan"]);
        assert_eq!(clean_key("\u{8}Da\u{8}Shāhī Dowlat [Afghanistan]"), vec!["Da Shāhī Dowlat", "Afghanistan"]);
    }

    #[test]
    fn antimeridian_bbox() {
        let mut e = Extent::default();
        e.add(60.0, 179.0);
        e.add(62.0, -178.0);
        let b = e.bbox().unwrap();
        assert!(b[0] > b[2] && (b[0] - 179.0).abs() < 1e-9 && (b[2] + 178.0).abs() < 1e-9);
        let (_, lon) = e.centre().unwrap();
        assert!((lon - 180.5).abs() < 1e-9 || (lon + 179.5).abs() < 1e-9);
    }
}
