//! Completeness: every section of every MAD file, converted (and where to) or omitted (and why).

use anyhow::Result;
use encarta_formats::mad::schemas::{GeoObjRecord, decode_record};
use encarta_formats::mad::{Atlas, Edition};
use rayon::prelude::*;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

/// Where each WLD section went.
fn disposition(name: &str) -> (&'static str, &'static str) {
    const LCID_TABLES: &[&str] =
        &["GeoObjGrid", "GeoObjLookup", "Find Grid", "Find Ndx", "Find Data", "Find Key", "FindGECContextGrid", "FindGECContextLookup"];
    let base = match name.len().checked_sub(4) {
        Some(i) if LCID_TABLES.contains(&&name[..i]) && name[i..].chars().all(|c| c.is_ascii_hexdigit()) => &name[..i],
        _ => name,
    };
    match base {
        "Terrain" => ("converted", "raster/*/*.pmtiles (every LOOK a MapStyle uses + class-index PNG), raster/*/*.equirect.*, raster/palettes.json. The TESS globe mesh is not converted: it is regenerable from the projection (mad.md §5.2)."),
        "GeoGemGrid" | "GeoGemLookup" => ("converted", "vector/earth.pmtiles, vector/earth-detail.pmtiles, vector/moon.pmtiles; extents in gazetteer.sqlite"),
        "GeoObjGrid" | "GeoObjLookup" => ("converted", "feature names/ids in vector tiles and gazetteer.sqlite (both LCIDs)"),
        "Find Grid" | "Find Ndx" => ("converted", "gazetteer.sqlite: feature type, context, search keys (place_name kind search_key, FTS5)"),
        "Find Data" => ("converted", "gazetteer.sqlite tables feature_type and context (the strings that Find Grid indexes). The type-taxonomy records and the context-node tree are not exported (mad-styling.md §13.4: not needed)."),
        "FindGECContextGrid" | "FindGECContextLookup" => ("omitted", "redundant: the (type, context) pairs of the Find Grid entries, which gazetteer.sqlite already carries (mad.md §7.5)"),
        "Find Key" => ("omitted", "page index of Find Grid for MOBB's binary search (collation weights of each page's first word); replaced by the FTS5 index. Collation data itself is exported (Font Conv)"),
        "EntityConv" | "EntityConvKey" | "EntityConvLookup" => ("converted", "used to join entities (entity → GeoObj record/entry); gazetteer.sqlite place.geoobj_key/geoobj_entry"),
        "EntityCenter" => ("converted", "gazetteer.sqlite place.lat/lon (position_source = entity_center)"),
        "PDCAttribute" => ("converted", "class ids/names in tiles; gazetteer.sqlite class; mad-tables.json"),
        "PDCAltitudeGrid" | "PDCAltitudeLookup" => ("converted", "styles/*.json layers; mad-tables.json"),
        "GeoPenDef" | "GeoLabel" | "Label Font" | "GeoTrans" => ("converted", "styles/*.json (pens, labels, fonts, tint approximations); mad-tables.json"),
        "MapStyle" => ("converted", "one styles/<name>.json per record; styles/index.json"),
        "MinRect" => ("converted", "style metadata encarta:min_rects; mad-tables.json"),
        "GeoFaceScale" => ("converted", "band → zoom mapping (vector tiles); mad-tables.json"),
        "Bitmap" | "BitmapLookup" | "BitmapPalette" => ("converted", "sprites/sprite{,@2x}.{png,json}"),
        "BitmapDimDir" => ("converted", "redundant with the Bitmap record sizes (validated identical); mad-tables.json"),
        "BitmapColor" => ("converted", "mad-tables.json (meaning open)"),
        "ShieldId" => ("converted", "mad-tables.json; no shield source in the world data yet (mad-styling.md §8)"),
        "LegendGrid" | "LegendLookup" => ("converted", "mad-tables.json"),
        "G1G2 to Unicode" => ("converted", "used to decode Find strings into gazetteer.sqlite"),
        "Unicode Attributes" | "Font Conv" => ("converted", "mad-tables.json (search collation data, for exact search parity)"),
        "LangId" => ("converted", "selects the GeoObj languages (0409 primary, 0009 as name_0009)"),
        "MillerStyles" | "MillerTerrainGrid" | "MillerTerrainLku" => ("converted", "miller/<raster>/index-<level>.png + <look>-<level>.webp, miller/index.json"),
        _ => ("unexplained", "no disposition recorded"),
    }
}

pub fn build(extracted: &Path, wld: &Atlas, stage_reports: &serde_json::Map<String, Value>) -> Result<Value> {
    let mut sections = Vec::new();
    for f in &wld.files {
        for s in &f.toc.sections {
            let (status, detail) = disposition(&s.name);
            sections.push(json!({ "file": f.path.file_name().unwrap().to_string_lossy(), "section": s.name, "bytes": s.length, "status": status, "detail": detail }));
        }
    }
    // SWLD: a lower-detail atlas that ENCARTA.EXE opens when WLD_CD is not installed.
    let mut swld = json!({ "status": "omitted" });
    if let Ok(s) = Atlas::open_edition(extracted, Edition::Student) {
        let ids = |a: &Atlas| -> Result<HashSet<u32>> {
            let g = a.geo_obj(0x0409)?;
            let keys: Vec<u32> = g.index.runs().iter().flat_map(|r| r.first..=r.last()).collect();
            let v: Vec<Vec<u32>> = keys.par_iter().map(|&k| Ok(decode_record::<GeoObjRecord>(&g, k)?.entries.iter().map(|e| e.entity).collect())).collect::<Result<_>>()?;
            Ok(v.into_iter().flatten().collect())
        };
        let (w, sw) = (ids(wld)?, ids(&s)?);
        let missing: Vec<u32> = sw.iter().filter(|i| !w.contains(i)).copied().take(50).collect();
        let n_missing = sw.iter().filter(|i| !w.contains(i)).count();
        let tiles: u64 = s.terrain()?.rasters.iter().map(|r| r.index.key_count()).sum();
        swld = json!({
            "status": "omitted",
            "reason": "SWLD_HD/SWLD_CD form the reduced atlas ENCARTA.EXE falls back to without WLD_CD (DELUXE.INI lists both). It has HYPSO levels 0–7 (a simplified rendering without bathymetry) and LOCATOR, GeoGem bands 0–7 only, and a subset of the WLD entities. The Premium atlas is WLD.",
            "entities": sw.len(), "entities_not_in_wld": n_missing, "examples_not_in_wld": missing,
            "raster_tiles": tiles,
            "sections": s.section_names(),
        });
    }
    let unexplained = sections.iter().filter(|s| s["status"] == "unexplained").count();
    Ok(json!({ "wld_sections": sections, "unexplained_sections": unexplained, "swld": swld, "stages": stage_reports.keys().collect::<Vec<_>>() }))
}
