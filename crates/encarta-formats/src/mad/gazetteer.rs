//! Entity lookups across the gazetteer tables (mad.md §7.10).

use super::cells::{Cell, CellSystem};
use super::g1g2::G1G2;
use super::grid::Grid;
use super::schemas::{EntityConvPage, EntityLocation, FindPage, GeoObjEntry, GeoObjRecord, decode_record};
use super::tables::{self, EntityCenter, FindData};
use super::{Atlas, lcid_name};
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde::Serialize;

/// Everything known about one entity.
#[derive(Debug, Clone, Serialize)]
pub struct EntityRecord {
    pub id: u32,
    /// Display name and alternatives (GeoObj names split at U+0004).
    pub names: Vec<String>,
    pub geoobj_key: u32,
    pub geoobj_entry: u16,
    pub cell: Option<Cell>,
    /// Cell bounds `(west, south, east, north)`.
    pub cell_bounds: Option<(f64, f64, f64, f64)>,
    pub class: u8,
    pub scale: u32,
    pub label: Option<(u32, u16, u16)>,
    /// Find Grid feature type and its description.
    pub find_type: Option<(u16, String)>,
    /// Find Grid administrative context and its text.
    pub context: Option<(u16, String)>,
    /// Search keys from Find Grid.
    pub search_keys: Vec<String>,
    /// EntityCenter centroid `(lat, lon)`.
    pub centroid: Option<(f64, f64)>,
}

pub struct Gazetteer<'a> {
    pub lcid: u16,
    conv: Grid<'a>,
    conv_first_ids: Vec<u32>,
    pub geo_obj: Grid<'a>,
    find: Option<Grid<'a>>,
    pub find_data: Option<FindData>,
    centers: Vec<EntityCenter>,
    pub cells: CellSystem,
    pub g1g2: G1G2,
}

impl<'a> Gazetteer<'a> {
    pub fn open(atlas: &'a Atlas, lcid: u16) -> Result<Gazetteer<'a>> {
        let keys = tables::key_table(atlas.require("EntityConvKey")?)?;
        let conv_first_ids = keys.iter().map(|k| k.value.context("EntityConvKey without values")).collect::<Result<_>>()?;
        let find = atlas.find_grid(lcid).ok();
        let find_data = if find.is_some() { Some(atlas.find_data(lcid)?) } else { None };
        let mut centers = atlas.entity_centers()?;
        centers.sort_by_key(|c| c.entity);
        Ok(Gazetteer {
            lcid,
            conv: atlas.grid("EntityConv", "EntityConvLookup")?,
            conv_first_ids,
            geo_obj: atlas.geo_obj(lcid)?,
            find,
            find_data,
            centers,
            cells: atlas.cells()?,
            g1g2: atlas.g1g2()?,
        })
    }

    /// Where an entity's GeoObj entry is (via EntityConvKey and EntityConv).
    pub fn locate(&self, id: u32) -> Result<Option<EntityLocation>> {
        let page = self.conv_first_ids.partition_point(|&f| f <= id);
        if page == 0 {
            return Ok(None);
        }
        let p: EntityConvPage = decode_record(&self.conv, page as u32 - 1)?;
        Ok(p.entries.binary_search_by_key(&id, |e| e.entity).ok().map(|i| p.entries[i]))
    }

    pub fn geo_obj_entry(&self, key: u32, entry: u16) -> Result<GeoObjEntry> {
        let rec: GeoObjRecord = decode_record(&self.geo_obj, key)?;
        rec.entries.into_iter().nth(entry as usize).with_context(|| format!("GeoObj {key:#x} has no entry {entry}"))
    }

    pub fn centroid(&self, id: u32) -> Option<(f64, f64)> {
        let i = self.centers.binary_search_by_key(&id, |c| c.entity).ok()?;
        let c = &self.centers[i];
        Some(self.cells.cell(c.cell_key)?.lat_lon(c.x as f64, c.y as f64))
    }

    /// All Find Grid entries of an entity (scans every page).
    pub fn find_entries(&self, id: u32) -> Result<Vec<super::schemas::FindEntry>> {
        let Some(find) = &self.find else { return Ok(Vec::new()) };
        let pages: Vec<u32> = find.index.runs().iter().flat_map(|r| r.first..=r.last()).collect();
        let found: Result<Vec<Vec<_>>> = pages
            .par_iter()
            .map(|&p| {
                let page: FindPage = decode_record(find, p)?;
                Ok(page.entries.into_iter().filter(|e| e.entity == id).collect())
            })
            .collect();
        Ok(found?.into_iter().flatten().collect())
    }

    pub fn entity(&self, id: u32) -> Result<EntityRecord> {
        let Some(loc) = self.locate(id)? else { bail!("entity {id} is not in EntityConv") };
        let obj = self.geo_obj_entry(loc.geoobj_key, loc.entry)?;
        if obj.entity != id {
            bail!("EntityConv sends {id} to GeoObj {:#x}/{}, which holds entity {}", loc.geoobj_key, loc.entry, obj.entity);
        }
        let cell = self.cells.cell(loc.geoobj_key);
        let finds = self.find_entries(id)?;
        let fd = self.find_data.as_ref();
        let first = finds.first();
        let find_type = first.map(|e| (e.kind, fd.and_then(|d| d.types.get(e.kind as usize).cloned()).unwrap_or_default()));
        let context = first.map(|e| (e.context, fd.and_then(|d| d.contexts.get(e.context as usize).cloned()).unwrap_or_default()));
        let mut search_keys: Vec<String> = finds.iter().map(|e| self.g1g2.decode(&e.key)).collect();
        search_keys.dedup();
        Ok(EntityRecord {
            id,
            names: obj.name.split('\u{4}').map(str::to_string).collect(),
            geoobj_key: loc.geoobj_key,
            geoobj_entry: loc.entry,
            cell,
            cell_bounds: cell.map(|c| c.bounds()),
            class: obj.class,
            scale: obj.scale,
            label: obj.label,
            find_type,
            context,
            search_keys,
            centroid: self.centroid(id),
        })
    }
}

/// The `Find Grid` section name for an LCID (for messages).
pub fn find_grid_name(lcid: u16) -> String {
    lcid_name("Find Grid", lcid)
}
