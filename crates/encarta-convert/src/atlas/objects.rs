//! The GeoObj table of both languages as one flat object list, addressed by the GeoGem object
//! reference `(record key, entry)`, plus the PDCAttribute classes with their visibility summary.

use anyhow::{Context, Result};
use encarta_formats::mad::Atlas;
use encarta_formats::mad::cells::CellSystem;
use encarta_formats::mad::schemas::{DisplayRules, GeoObjRecord, ObjectRef, decode_record};
use encarta_formats::mad::tables::{self, FeatureClass, GeometryKind};
use rayon::prelude::*;
use std::collections::HashMap;

/// A display name split by the GeoObj conventions (mad-styling.md §7.3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Name {
    /// First alternative, without its abbreviation.
    pub primary: String,
    /// Abbreviation after U+0002, if any.
    pub abbr: Option<String>,
    /// Further alternatives after U+0004.
    pub alternatives: Vec<String>,
    /// `{RESERVED}…` placeholder names of anonymous features.
    pub anonymous: bool,
    pub raw: String,
}

pub fn split_name(raw: &str) -> Name {
    let mut alts = raw.split('\u{4}');
    let first = alts.next().unwrap_or_default();
    let (primary, abbr) = match first.split_once('\u{2}') {
        Some((p, a)) => (p.to_string(), Some(a.to_string()).filter(|a| !a.is_empty())),
        None => (first.to_string(), None),
    };
    let anonymous = primary.starts_with("{RESERVED}");
    Name {
        primary: if anonymous { String::new() } else { primary },
        abbr,
        alternatives: alts.filter(|a| !a.is_empty()).map(|a| a.split('\u{2}').next().unwrap_or(a).to_string()).collect(),
        anonymous,
        raw: raw.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Obj {
    pub entity: u32,
    pub name: Name,
    /// Name in the second language (LCID 0x0009), when it differs.
    pub name_alt_lcid: Option<String>,
    /// GeoObj `c`: "go to" altitude in km (0 = use the class altitude).
    pub goto_km: u32,
    /// GeoObj `(e, f, g)`: the "go to" point as (cell key, x, y).
    pub goto_cell: Option<(u32, u16, u16)>,
}

impl Obj {
    /// "Go to" point (lat, lon).
    pub fn goto_point(&self, cells: &CellSystem) -> Option<(f64, f64)> {
        let (key, x, y) = self.goto_cell?;
        Some(cells.cell(key)?.lat_lon(x as f64, y as f64))
    }
}

pub struct Objects {
    pub objs: Vec<Obj>,
    start: HashMap<u32, u32>,
    /// `(first object index, record key)`, ascending.
    by_start: Vec<(u32, u32)>,
    pub lcids: Vec<u16>,
}

impl Objects {
    pub fn load(atlas: &Atlas) -> Result<Objects> {
        let lcids = atlas.languages()?;
        let primary = *lcids.iter().find(|&&l| l == 0x0409).or(lcids.first()).context("no language")?;
        let grid = atlas.geo_obj(primary)?;
        let keys: Vec<u32> = grid.index.runs().iter().flat_map(|r| r.first..=r.last()).collect();
        let recs: Vec<(u32, GeoObjRecord)> =
            keys.par_iter().map(|&k| Ok((k, decode_record::<GeoObjRecord>(&grid, k)?))).collect::<Result<_>>()?;
        let mut others: Vec<HashMap<u32, GeoObjRecord>> = Vec::new();
        for &l in lcids.iter().filter(|&&l| l != primary) {
            let g = atlas.geo_obj(l)?;
            let ks: Vec<u32> = g.index.runs().iter().flat_map(|r| r.first..=r.last()).collect();
            others.push(ks.par_iter().map(|&k| Ok((k, decode_record::<GeoObjRecord>(&g, k)?))).collect::<Result<_>>()?);
        }
        let mut objs = Vec::new();
        let mut start = HashMap::new();
        for (k, rec) in recs {
            start.insert(k, objs.len() as u32);
            for (i, e) in rec.entries.into_iter().enumerate() {
                let alt = others
                    .iter()
                    .filter_map(|o| o.get(&k).and_then(|r| r.entries.get(i)))
                    .find(|a| a.entity == e.entity && a.name != e.name)
                    .map(|a| a.name.clone());
                objs.push(Obj { entity: e.entity, name: split_name(&e.name), name_alt_lcid: alt, goto_km: e.scale, goto_cell: e.label });
            }
        }
        let mut by_start: Vec<(u32, u32)> = start.iter().map(|(&k, &s)| (s, k)).collect();
        by_start.sort_unstable();
        Ok(Objects { objs, start, by_start, lcids })
    }

    pub fn index(&self, r: ObjectRef) -> Option<u32> {
        let s = *self.start.get(&r.key)?;
        let i = s + r.entry;
        (i < self.objs.len() as u32).then_some(i)
    }

    /// GeoObj `(record key, entry)` of object `i`.
    pub fn location(&self, i: u32) -> (u32, u32) {
        let j = self.by_start.partition_point(|&(s, _)| s <= i) - 1;
        let (s, k) = self.by_start[j];
        (k, i - s)
    }

}

/// Hit side 2, "water", of an interactive rule (PDCAltitudeGrid `b4` high nibble, mad-styling.md
/// §4.4): listed only where the point is over water. 0 is anywhere; 1, "land", is left out when a
/// water hit is listed too.
pub const HIT_SIDE_WATER: u8 = 2;

/// How each class is used by the display rules of all MapStyles (mad-styling.md §4).
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassUse {
    /// Rules whose geometry tri-state is 1 (shown by default) / 2 (optional).
    pub geometry_default: u32,
    pub geometry_optional: u32,
    pub label: u32,
    pub interactive: u32,
    /// Area rules that stroke the outline (a pen in w8…w13, or a stroke highlight b16 = 1/2):
    /// the class also gets seam-free outlines (layer `outlines`).
    pub outline: u32,
    /// Line rules with a pen whose "draw over" mask is not everything (mad-styling.md §5.2): the
    /// class's lines are split into runs over water and land.
    pub masked: u32,
    /// Area rules whose fill pen or pattern brush has a "draw over" mask that is not everything:
    /// the class's polygons are split into parts over water and land.
    pub masked_fill: u32,
    /// Area rules whose fill may draw over water in their style's raster (its water pixel class
    /// is in the pen's mask).
    pub fill_water: u32,
    /// Interactive Earth rules whose hit side (`b4` high nibble, mad-styling.md §4.4) is 2, "water":
    /// the class is listed only where the point is over water, so its polygons are split into
    /// water and land parts.
    pub water_hits: u32,
}

impl ClassUse {
    pub fn geometry(&self) -> bool {
        self.geometry_default + self.geometry_optional > 0
    }
    /// Geometry needed in tiles: drawn, or used for hit-testing (hover, click).
    pub fn needs_geometry(&self) -> bool {
        self.geometry() || self.interactive > 0
    }
    /// Water parts of a split area class are drawn by some style or hit-tested (the click menu
    /// lists land classes over the sea unless a water class is hit there too, mad-styling.md §4.4).
    pub fn needs_water_parts(&self) -> bool {
        self.fill_water > 0 || self.interactive > 0
    }
    pub fn anything(&self) -> bool {
        self.needs_geometry() || self.label > 0
    }
}

pub struct Classes {
    pub classes: Vec<FeatureClass>,
    pub kinds: Vec<GeometryKind>,
    pub uses: Vec<ClassUse>,
}

impl Classes {
    pub fn load(atlas: &Atlas) -> Result<Classes> {
        let classes = atlas.classes()?;
        let kinds = classes.iter().map(|c| c.kind).collect();
        let mut uses = vec![ClassUse::default(); classes.len()];
        let grid = atlas.grid("PDCAltitudeGrid", "PDCAltitudeLookup")?;
        let pens = tables::pen_defs(atlas.require("GeoPenDef")?)?;
        let pen = |w: u16| (w < 0xFEF9).then(|| pens.get(w as usize)).flatten();
        let terrain = atlas.terrain()?;
        let hypso = terrain.raster("HYPSO").context("no HYPSO raster")?;
        for s in atlas.map_styles()? {
            let rules: DisplayRules = decode_record(&grid, s.id as u32)?;
            // The pixel classes under water in this style's raster (Moon styles are not split).
            let surface = if s.raster.eq_ignore_ascii_case("MOON") {
                None
            } else {
                let raster = terrain.raster(&s.raster).with_context(|| format!("raster {} of style {}", s.raster, s.name))?;
                let look = raster.look(&s.look).with_context(|| format!("LOOK {} of style {}", s.look, s.name))?;
                Some(super::pixel::calibrate(hypso, raster, look, 4096)?)
            };
            for r in &rules.rules {
                let Some(u) = uses.get_mut(r.class as usize) else { continue };
                match r.b2 & 0x0F {
                    1 => u.geometry_default += 1,
                    2 => u.geometry_optional += 1,
                    _ => {}
                }
                if r.b3 != 0 {
                    u.label += 1;
                }
                if r.b4 & 0x0F != 0 {
                    u.interactive += 1;
                    if r.b4 >> 4 == HIT_SIDE_WATER && surface.is_some() {
                        u.water_hits += 1;
                    }
                }
                let geometry = r.b2 & 0x0F != 0;
                match classes[r.class as usize].kind {
                    GeometryKind::Area => {
                        let stroke = geometry && r.w5_13[3..9].iter().any(|&w| pen(w).is_some_and(|p| p.b8[1] > 0));
                        let highlight = r.b14_16[1] == 1 && r.b14_16[2] != 3 && pen(r.w17).is_some();
                        if stroke || highlight {
                            u.outline += 1;
                        }
                        if geometry && pen(r.w5_13[2]).is_some_and(|p| super::pixel::pen_mask(&p.flags) != super::pixel::ALL_BITS) {
                            u.masked_fill += 1;
                        }
                        if geometry && pen(r.w5_13[2]).is_some_and(|p| surface.is_some_and(|sf| sf.pen_sides(super::pixel::pen_mask(&p.flags)).0)) {
                            u.fill_water += 1;
                        }
                    }
                    GeometryKind::Line
                        if geometry && r.w5_13[2..9].iter().any(|&w| pen(w).is_some_and(|p| p.b8[1] > 0 && super::pixel::pen_mask(&p.flags) != super::pixel::ALL_BITS)) =>
                    {
                        u.masked += 1;
                    }
                    _ => {}
                }
            }
        }
        Ok(Classes { classes, kinds, uses })
    }

    /// Lunar features (MOON raster) live in the same cell system as the Earth's.
    pub fn is_moon(&self, row: usize) -> bool {
        self.classes[row].name.starts_with("MOON")
    }

    /// Interactive Earth area classes that the click menu lists only over water (hit side 2 in
    /// some style): their polygons are split into water and land parts, so that the client can
    /// skip the land parts.
    pub fn water_hits(&self, row: usize) -> bool {
        self.kinds[row] == GeometryKind::Area && self.uses[row].water_hits > 0 && !self.is_moon(row)
    }
}

#[cfg(test)]
impl Objects {
    /// Objects with entity ids 1000, 1001, … and the given names (one record, key 0).
    pub fn synthetic(names: &[&str]) -> Objects {
        let objs = names
            .iter()
            .enumerate()
            .map(|(i, n)| Obj { entity: 1000 + i as u32, name: split_name(n), name_alt_lcid: None, goto_km: 0, goto_cell: None })
            .collect();
        Objects { objs, start: HashMap::from([(0, 0)]), by_start: vec![(0, 0)], lcids: vec![0x0409] }
    }
}

#[cfg(test)]
impl Classes {
    pub fn synthetic(kinds: &[GeometryKind]) -> Classes {
        let classes = kinds
            .iter()
            .enumerate()
            .map(|(i, &kind)| FeatureClass { id: i as u16, kind, pen_group: 0, flag: 0, scale: 0, reserved: 0, group: 0, priority: 50, name: format!("class {i}") })
            .collect();
        Classes { classes, kinds: kinds.to_vec(), uses: vec![ClassUse { geometry_default: 1, geometry_optional: 0, label: 1, interactive: 1, outline: 1, masked: 1, masked_fill: 1, fill_water: 1, water_hits: 0 }; kinds.len()] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_conventions() {
        let n = split_name("United Kingdom\u{2}U.K.");
        assert_eq!((n.primary.as_str(), n.abbr.as_deref()), ("United Kingdom", Some("U.K.")));
        let n = split_name("Danube\u{4}Donau");
        assert_eq!((n.primary.as_str(), n.alternatives.clone()), ("Danube", vec!["Donau".to_string()]));
        let n = split_name("{RESERVED}Anonymous Earthquake");
        assert!(n.anonymous && n.primary.is_empty());
    }
}
