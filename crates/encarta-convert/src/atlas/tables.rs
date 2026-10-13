//! The small MAD tables as JSON (`out/atlas/mad-tables.json`) for the client and for later
//! styling work, and the Miller flat maps (`out/atlas/miller/`).

use super::raster::{png_gray, webp_lossless, webp_lossy};
use super::style::min_rects;
use super::{geo, objects::Classes};
use anyhow::{Context, Result, ensure};
use encarta_formats::mad::Atlas;
use encarta_formats::mad::schemas::{DisplayRules, Legend, MillerTile, decode_record};
use encarta_formats::mad::tables;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::Path;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// `Unicode Attributes`: `{u16 code point, u16 upper, u16 base letter, u8 ANSI fallback}`.
fn unicode_attributes(b: &[u8]) -> Value {
    json!(b.as_chunks::<7>().0.iter().map(|r| json!([u16_at(r, 0), u16_at(r, 2), u16_at(r, 4), r[6]])).collect::<Vec<_>>())
}

/// `Font Conv`: 3 maps × 256 × `{u16 base, u16 weight, u8 letter, u8 separator}`.
fn font_conv(b: &[u8]) -> Value {
    json!(b.chunks(256 * 6).map(|m| m.as_chunks::<6>().0.iter().map(|r| json!([u16_at(r, 0), u16_at(r, 2), r[4], r[5]])).collect::<Vec<_>>()).collect::<Vec<_>>())
}

pub fn export(atlas: &Atlas, classes: &Classes, out: &Path) -> Result<Vec<&'static str>> {
    let mut m = serde_json::Map::new();
    let mut done = Vec::new();
    let mut put = |k: &'static str, v: Value| {
        m.insert(k.into(), v);
        done.push(k);
    };
    let cells = atlas.cells()?;
    put("GeoFaceScale", json!(cells.bands.iter().map(|b| json!({
        "max_altitude_km": b.max_scale as f64 / 1000.0, "min_altitude_km": b.min_scale as f64 / 1000.0, "cells_per_octant_edge": b.n,
        "first_key": b.first_key, "zoom_from": geo::zoom_for_altitude(b.max_scale as f64 / 1000.0), "zoom_to": geo::zoom_for_altitude(b.min_scale as f64 / 1000.0).min(24.0) })).collect::<Vec<_>>()));
    put("PDCAttribute", serde_json::to_value(&classes.classes)?);
    put("ClassUse", json!(classes.uses.iter().map(|u| json!([u.geometry_default, u.geometry_optional, u.label, u.interactive])).collect::<Vec<_>>()));
    put("GeoPenDef", serde_json::to_value(tables::pen_defs(atlas.require("GeoPenDef")?)?)?);
    put("GeoLabel", serde_json::to_value(tables::label_styles(atlas.require("GeoLabel")?)?)?);
    put("Label Font", serde_json::to_value(tables::label_fonts(atlas.require("Label Font")?)?)?);
    put("ShieldId", serde_json::to_value(tables::shield_ids(atlas.require("ShieldId")?)?)?);
    put("BitmapPalette", serde_json::to_value(atlas.bitmap_palette()?)?);
    if let Some(b) = atlas.section("BitmapColor") {
        put("BitmapColor", serde_json::to_value(tables::bitmap_color(b)?)?);
    }
    put("BitmapDimDir", serde_json::to_value(tables::bitmap_dims(atlas.require("BitmapDimDir")?)?)?);
    let gt = atlas.require("GeoTrans")?;
    put("GeoTrans", json!(gt.as_chunks::<45>().0.iter().map(|r| r.as_chunks::<3>().0.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>()).collect::<Vec<_>>()));
    put("MinRect", json!(min_rects(atlas.require("MinRect")?).iter().map(|r| json!({ "style": r.0, "altitude_m": r.1, "bbox": r.2 })).collect::<Vec<_>>()));
    put("Unicode Attributes", unicode_attributes(atlas.require("Unicode Attributes")?));
    put("Font Conv", font_conv(atlas.require("Font Conv")?));
    let lg = atlas.grid("LegendGrid", "LegendLookup")?;
    let mut legends = serde_json::Map::new();
    for r in lg.index.runs() {
        for k in r.first..=r.last() {
            legends.insert(format!("{k:04x}"), serde_json::to_value(decode_record::<Legend>(&lg, k)?)?);
        }
    }
    put("LegendGrid", Value::Object(legends));
    let ms = atlas.map_styles()?;
    put("MapStyle", json!(ms.iter().map(|s| json!({ "id": s.id, "name": s.name, "raster": s.raster, "look": s.look,
        "raw_hex": s.raw.iter().map(|b| format!("{b:02x}")).collect::<String>() })).collect::<Vec<_>>()));
    let pg = atlas.grid("PDCAltitudeGrid", "PDCAltitudeLookup")?;
    let mut rules = serde_json::Map::new();
    for s in &ms {
        rules.insert(s.id.to_string(), serde_json::to_value(decode_record::<DisplayRules>(&pg, s.id as u32)?)?);
    }
    put("PDCAltitudeGrid", Value::Object(rules));
    put("LangId", json!(atlas.languages()?));
    std::fs::write(out.join("mad-tables.json"), serde_json::to_vec(&Value::Object(m))?)?;
    Ok(done)
}

#[derive(Debug, Default, Serialize)]
pub struct MillerReport {
    pub levels: Vec<(u32, u32)>,
    pub images: Vec<String>,
    pub records: u32,
}

/// The flat maps: one image per raster and level (mad-styling.md §12).
pub fn miller(atlas: &Atlas, out: &Path) -> Result<MillerReport> {
    let mut rep = MillerReport::default();
    let Some(ms) = atlas.section("MillerStyles") else { return Ok(rep) };
    let n = ms[6] as usize;
    rep.levels = (0..n).map(|l| (u32::from_le_bytes(ms[7 + 8 * l..11 + 8 * l].try_into().unwrap()), u32::from_le_bytes(ms[11 + 8 * l..15 + 8 * l].try_into().unwrap()))).collect();
    let entries = tables::miller_styles(ms)?.entries;
    let grid = atlas.grid("MillerTerrainGrid", "MillerTerrainLku")?;
    let terrain = atlas.terrain()?;
    let mut key = 0u32;
    let mut meta = Vec::new();
    for e in &entries {
        let raster = terrain.raster(&e.raster).with_context(|| format!("Miller raster {}", e.raster))?;
        for level in 0..=e.levels as usize {
            let (w, h) = rep.levels[level];
            let (cols, rows) = (w.div_ceil(256), h.div_ceil(255));
            let mut img = vec![0u8; (w * h) as usize];
            for _ in 0..cols * rows {
                let t: MillerTile = decode_record(&grid, key)?;
                key += 1;
                let s = t.slot as u32;
                let (mut col, mut row) = (0u32, 0u32);
                for b in 0..8 {
                    col |= ((s >> (2 * b)) & 1) << b;
                    row |= ((s >> (2 * b + 1)) & 1) << b;
                }
                ensure!(col < cols && row < rows, "Miller slot {s} outside {cols}×{rows}");
                for y in 0..t.height as u32 {
                    for x in 0..t.width as u32 {
                        let (ix, iy) = (col * 256 + x, row * 255 + y);
                        if ix < w && iy < h {
                            img[(iy * w + ix) as usize] = t.pixels[(y * t.width as u32 + x) as usize];
                        }
                    }
                }
            }
            let dir = out.join("miller").join(e.raster.to_ascii_lowercase());
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join(format!("index-{level}.png")), png_gray(&img, w, h))?;
            rep.images.push(format!("miller/{}/index-{level}.png", e.raster.to_ascii_lowercase()));
            for look in &e.looks {
                let Some(l) = raster.look(look) else { continue };
                let rgb: Vec<u8> = img.iter().flat_map(|&v| l.values[v as usize].rgb).collect();
                let lossy = matches!(e.raster.to_ascii_uppercase().as_str(), "SATELLITE" | "NIGHT" | "MOON");
                let bytes = if lossy { webp_lossy(&rgb, w, h) } else { webp_lossless(&rgb, w, h) };
                let name = format!("{}-{level}.webp", look.to_ascii_lowercase());
                std::fs::write(dir.join(&name), bytes)?;
                rep.images.push(format!("miller/{}/{name}", e.raster.to_ascii_lowercase()));
            }
            meta.push(json!({ "raster": e.raster, "level": level, "width": w, "height": h, "min_altitude_km": e.scale,
                "projection": "Miller cylindrical: x = W/2 + k·lon, y = H/2 − k·(180/π)·1.25·ln tan(π/4 + 0.4·lat), k = W/360 (mad-styling.md §12.3; y0 = H/2 is fitted, not exact)" }));
        }
    }
    rep.records = key;
    ensure!(key as usize == grid.index.records().len(), "Miller: {key} records used of {}", grid.index.records().len());
    std::fs::write(out.join("miller/index.json"), serde_json::to_vec_pretty(&meta)?)?;
    Ok(rep)
}
