//! The tour's content record (tours-3d.md §4.3): Tour Stops with their camera commands, the
//! default stop, the media each hotspot opens (`destmedia`), and the site maps (each stop's
//! `children/site` record: the map bitmap and its named locations, §4.2). These live in the
//! catalog's DATA records (group `virttour`), which `encarta-convert content` has already decoded
//! into `out/content.sqlite`; this module reads them back from there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

/// One Tour Stop, in menu order.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    pub id: u64,
    pub title: String,
    /// `default/@coord3d`: the command string the host sends to move to the stop (disc coordinates).
    pub command: Option<String>,
    /// Hotspot content ids of the stop, in menu order.
    pub hotspots: Vec<u64>,
    /// `children/site`: the site map shown while at this stop (one per floor in multi-floor tours).
    pub site: Option<u64>,
    /// `children/article`: the stop text shown below the site map.
    pub article: Option<u64>,
}

/// A site map: the stop's `children/site` record (tours-3d.md §4.2).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: u64,
    /// The record's image (a baggage DIB, e.g. `00068668.dib`, 344×194: two 172×194 halves, the
    /// plain map and the map as tinted inside the view wedge).
    pub image_file: Option<String>,
    /// The named map locations, in record order.
    pub locations: Vec<SiteLocation>,
    /// The image's bytes, found in the extracted disc's baggage by the caller.
    #[serde(skip)]
    pub image_data: Option<Vec<u8>>,
}

/// A named location on a site map: a `virttour` record with `destmedia` = a stop and `x`, `y` =
/// the top-left pixel of its box in the site-map window.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteLocation {
    pub id: u64,
    pub title: String,
    pub stop: Option<u64>,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

/// What a hotspot click opens.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotspotTarget {
    pub id: u64,
    pub title: String,
    /// `destmedia` of the hotspot's DATA record: the content id the host opens in its main view.
    pub dest_media: Option<u64>,
    /// The destination's DATA `group` (`media`, `flash`, …) and our item kind and title, when known.
    pub dest_group: Option<String>,
    pub dest_kind: Option<String>,
    pub dest_title: Option<String>,
    /// The stops that list this hotspot.
    pub stops: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TourContent {
    /// The tour record's title as Encarta shows it under the view, e.g. "Acropolis, 400 BC".
    pub title: Option<String>,
    pub default_stop: Option<u64>,
    pub stops: Vec<Stop>,
    pub hotspots: Vec<HotspotTarget>,
    /// The tour record's `children/article` (the credits, for every shipped tour).
    pub article: Option<u64>,
    /// Every site map a stop names, in first-use order.
    pub sites: Vec<Site>,
}

impl TourContent {
    pub fn target(&self, hotspot: u64) -> Option<&HotspotTarget> {
        self.hotspots.iter().find(|h| h.id == hotspot)
    }
}

struct Item {
    kind: String,
    group: Option<String>,
    title: String,
    meta: Value,
}

fn item(db: &Connection, id: u64) -> Result<Option<Item>> {
    Ok(db
        .query_row("SELECT kind, source_group, COALESCE(title, ''), meta_json FROM item WHERE ref_id = ?1", [id as i64], |r| {
            Ok(Item {
                kind: r.get(0)?,
                group: r.get(1)?,
                title: r.get(2)?,
                meta: r.get::<_, Option<String>>(3)?.and_then(|m| serde_json::from_str(&m).ok()).unwrap_or(Value::Null),
            })
        })
        .optional()?)
}

fn children(db: &Connection, id: u64, kind: &str) -> Result<Vec<u64>> {
    let mut st = db.prepare_cached("SELECT child_ref_id FROM item_child WHERE ref_id = ?1 AND kind = ?2 ORDER BY ordinal")?;
    let rows = st.query_map(rusqlite::params![id as i64, kind], |r| r.get::<_, i64>(0))?;
    Ok(rows.filter_map(|r| r.ok()).map(|v| v as u64).collect())
}

fn image_file(db: &Connection, id: u64) -> Result<Option<String>> {
    Ok(db
        .query_row("SELECT file_name FROM item_file WHERE ref_id = ?1 AND role = 'image' ORDER BY ordinal LIMIT 1", [id as i64], |r| r.get(0))
        .optional()?)
}

fn read_site(db: &Connection, id: u64) -> Result<Site> {
    let mut locations = Vec::new();
    for l in children(db, id, "hotspot")? {
        let Some(rec) = item(db, l)? else { continue };
        let attrs = &rec.meta["source_attrs"];
        let px = |k: &str| as_id(&attrs[k]).and_then(|v| i32::try_from(v).ok());
        locations.push(SiteLocation { id: l, title: rec.title, stop: as_id(&attrs["destmedia"]), x: px("x"), y: px("y") });
    }
    Ok(Site { id, image_file: image_file(db, id)?, locations, image_data: None })
}

/// A JSON value that is a number or a numeric string.
fn as_id(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Reads the content record of the tour `content_id` (e.g. 701504257). `None` when the item is
/// not in the database.
pub fn read(db: &Connection, content_id: u64) -> Result<Option<TourContent>> {
    let Some(tour) = item(db, content_id)? else { return Ok(None) };
    let mut out = TourContent {
        title: Some(tour.title.clone()).filter(|t| !t.is_empty()),
        default_stop: as_id(&tour.meta["panorama"]["refid"]),
        article: children(db, content_id, "article")?.first().copied(), ..Default::default() };
    let mut seen = BTreeSet::new();
    for stop_id in children(db, content_id, "media")? {
        let Some(stop) = item(db, stop_id)? else { continue };
        let hotspots = children(db, stop_id, "hotspot")?;
        for &h in &hotspots {
            if seen.insert(h) {
                let rec = item(db, h)?;
                let dest = rec.as_ref().and_then(|r| as_id(&r.meta["source_attrs"]["destmedia"]));
                let dest_item = match dest {
                    Some(d) => item(db, d)?,
                    None => None,
                };
                out.hotspots.push(HotspotTarget {
                    id: h,
                    title: rec.map(|r| r.title).unwrap_or_default(),
                    dest_media: dest,
                    dest_group: dest_item.as_ref().and_then(|d| d.group.clone()),
                    dest_kind: dest_item.as_ref().map(|d| d.kind.clone()),
                    dest_title: dest_item.map(|d| d.title),
                    stops: Vec::new(),
                });
            }
            if let Some(t) = out.hotspots.iter_mut().find(|t| t.id == h) {
                t.stops.push(stop_id);
            }
        }
        let site = children(db, stop_id, "site")?.first().copied();
        if let Some(s) = site
            && !out.sites.iter().any(|x| x.id == s)
        {
            out.sites.push(read_site(db, s)?);
        }
        out.stops.push(Stop {
            id: stop_id,
            title: stop.title,
            command: stop.meta["panorama"]["coord3d"].as_str().map(str::to_string),
            hotspots,
            site,
            article: children(db, stop_id, "article")?.first().copied(),
        });
    }
    Ok(Some(out))
}

/// Reads the content records of several tours from `out/content.sqlite`. An absent database
/// gives an empty map (the tours then convert without stops and hotspot targets).
pub fn read_all(db_path: &Path, content_ids: &[u64]) -> Result<BTreeMap<u64, TourContent>> {
    let mut out = BTreeMap::new();
    if !db_path.is_file() {
        return Ok(out);
    }
    let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    for &id in content_ids {
        if let Some(c) = read(&db, id)? {
            out.insert(id, c);
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A synthetic catalog: one tour, two stops, three hotspots (one without a destination).
    pub(crate) fn synthetic_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE item (ref_id INTEGER PRIMARY KEY, kind TEXT, source_group TEXT, title TEXT, meta_json TEXT);
             CREATE TABLE item_child (ref_id INTEGER, ordinal INTEGER, kind TEXT, child_ref_id INTEGER, sequence INTEGER);
             CREATE TABLE item_file (ref_id INTEGER, ordinal INTEGER, role TEXT, file_name TEXT);
             INSERT INTO item_file VALUES (800, 0, 'image', 'site800.dib');",
        )
        .unwrap();
        let items: &[(i64, &str, &str, &str, &str)] = &[
            (100, "virtual-tour-node", "virttour", "Test Tour", r#"{"panorama":{"refid":102}}"#),
            (101, "virtual-tour-node", "virttour", "First Stop", r#"{"panorama":{"coord3d":"CAMERA.MOVETO=1,0,2,90;CAMERA.PITCH=5;"}}"#),
            (102, "virtual-tour-node", "virttour", "Second Stop", r#"{"panorama":{"coord3d":"CAMERA.MOVETO=9,9,9,9;CAMERA.MOVETO=3,0,4,180;"}}"#),
            (201, "virtual-tour-node", "virttour", "A Frieze", r#"{"source_attrs":{"destmedia":"301","x":"1.5"}}"#),
            (202, "virtual-tour-node", "virttour", "Then and Now", r#"{"source_attrs":{"destmedia":"302"}}"#),
            (203, "virtual-tour-node", "virttour", "Nowhere", r#"{"source_attrs":{}}"#),
            (301, "media", "media", "A Frieze, Detail", "{}"),
            (302, "interactive", "flash", "Then and Now", "{}"),
            (800, "virtual-tour", "vtour", "", "{}"),
            (801, "virtual-tour-node", "virttour", "First Stop", r#"{"source_attrs":{"destmedia":"101","x":"12","y":"34"}}"#),
            (802, "virtual-tour-node", "virttour", "Second Stop", r#"{"source_attrs":{"destmedia":"102"}}"#),
        ];
        for (id, kind, group, title, meta) in items {
            db.execute("INSERT INTO item VALUES (?1, ?2, ?3, ?4, ?5)", rusqlite::params![id, kind, group, title, meta]).unwrap();
        }
        // Children in a non-sorted insertion order; menu order is the ordinal.
        let kids: &[(i64, i64, &str, i64)] = &[
            (100, 2, "media", 102),
            (100, 0, "media", 101),
            (100, 1, "article", 900),
            (101, 1, "hotspot", 202),
            (101, 0, "hotspot", 201),
            (101, 2, "site", 800),
            (102, 0, "hotspot", 203),
            (102, 1, "hotspot", 201),
            (102, 2, "article", 902),
            (102, 3, "site", 800),
            (800, 1, "hotspot", 802),
            (800, 0, "hotspot", 801),
        ];
        for (p, o, k, c) in kids {
            db.execute("INSERT INTO item_child VALUES (?1, ?2, ?3, ?4, NULL)", rusqlite::params![p, o, k, c]).unwrap();
        }
        db
    }

    #[test]
    fn reads_stops_default_and_hotspot_destinations() {
        let db = synthetic_db();
        let c = read(&db, 100).unwrap().unwrap();
        assert_eq!(c.default_stop, Some(102));
        assert_eq!(c.stops.iter().map(|s| s.id).collect::<Vec<_>>(), [101, 102]);
        assert_eq!(c.stops[0].command.as_deref(), Some("CAMERA.MOVETO=1,0,2,90;CAMERA.PITCH=5;"));
        assert_eq!(c.stops[0].hotspots, [201, 202]);
        assert_eq!(c.stops[1].hotspots, [203, 201]);
        let a = c.target(201).unwrap();
        assert_eq!((a.dest_media, a.dest_group.as_deref(), a.dest_kind.as_deref()), (Some(301), Some("media"), Some("media")));
        assert_eq!(a.stops, [101, 102]);
        assert_eq!(c.target(202).unwrap().dest_group.as_deref(), Some("flash"));
        assert_eq!(c.target(203).unwrap().dest_media, None);
        assert!(read(&db, 999).unwrap().is_none());
    }

    #[test]
    fn reads_site_maps_and_articles() {
        let db = synthetic_db();
        let c = read(&db, 100).unwrap().unwrap();
        assert_eq!(c.article, Some(900));
        assert_eq!(c.title.as_deref(), Some("Test Tour"));
        assert_eq!((c.stops[0].site, c.stops[0].article), (Some(800), None));
        assert_eq!((c.stops[1].site, c.stops[1].article), (Some(800), Some(902)));
        assert_eq!(c.sites.len(), 1, "a site shared by two stops is listed once");
        let s = &c.sites[0];
        assert_eq!((s.id, s.image_file.as_deref()), (800, Some("site800.dib")));
        assert_eq!(s.locations, [
            SiteLocation { id: 801, title: "First Stop".into(), stop: Some(101), x: Some(12), y: Some(34) },
            SiteLocation { id: 802, title: "Second Stop".into(), stop: Some(102), x: None, y: None },
        ]);
    }
}
