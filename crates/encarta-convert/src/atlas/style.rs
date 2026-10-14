//! MapLibre styles, one per Encarta MapStyle, compiled from the PDCAltitudeGrid display rules
//! (mad-styling.md §4, §5, §7, §9, §14), and the symbol sprite sheet from the Bitmap table.
//!
//! Rule → layer: the rule's altitude range becomes a zoom range (`geo::zoom_for_altitude`);
//! areas become fills (pen w7: colour, pattern bitmap, or a GeoTrans tint approximated by a
//! translucent fill), lines one layer per pen pass w7…w13 (width, dashes, offset, edge-only
//! pens), points symbol layers with the w7 pen's bitmap, labels symbol layers styled from GeoLabel
//! w5 with the rule's halo colour. Tri-state 1 → visible, 2 → `visibility: none` (optional).
//! Layers with identical paint, zoom range and visibility are merged with a class filter.
//! What the spec does not settle yet is marked with `"metadata": {"encarta:placeholder": …}`.

use super::geo;
use super::objects::Classes;
use super::pixel::{self, SurfaceClasses};
use super::vector;
use super::vector::Tileset;
use anyhow::{Context, Result};
use encarta_formats::mad::Atlas;
use encarta_formats::mad::cells::CellSystem;
use encarta_formats::mad::schemas::{Bitmap, DisplayRule, DisplayRules, decode_record};
use encarta_formats::mad::tables::{self, GeometryKind, LabelStyle, MapStyle, PenDef};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const NONE: u16 = 0xFFFF;
const SPECIAL_NONE: u16 = 0xFEFA;
const SPECIAL_PEN: u16 = 0xFEF9;

fn pen_ref(v: u16) -> Option<usize> {
    (v != NONE && v != SPECIAL_NONE && v != SPECIAL_PEN).then_some(v as usize)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn slug(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            o.push(ch.to_ascii_lowercase());
        } else if !o.ends_with('-') {
            o.push('-');
        }
    }
    o.trim_matches('-').to_string()
}

/// A MapStyle record and how the converter reads its colours (mad-styling.md §3: colour 1 is taken
/// as the space colour and colour 2 as the globe background, both hypotheses).
pub struct StyleInfo {
    pub style: MapStyle,
}

impl StyleInfo {
    fn space(&self) -> [u8; 3] {
        self.style.colour1
    }
    fn globe(&self) -> [u8; 3] {
        self.style.colour2
    }
    pub fn slug(&self) -> String {
        slug(&self.style.name)
    }
}

/// Glyph face name for a Label Font id and style flags ("MS Reference Sans Serif Bold").
pub fn face_name(fonts: &BTreeMap<u8, String>, font: u8, bold: bool, italic: bool) -> String {
    let family = fonts.get(&font).cloned().unwrap_or_else(|| "Arial".into());
    let style = match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    };
    format!("{family} {style}")
}

/// MapLibre dash array (in line widths) for a 32-bit dash mask with period `plen`.
fn dasharray(pattern: u32, plen: u8, width: f64) -> Option<Vec<f64>> {
    let plen = plen.clamp(1, 32) as u32;
    let bits: Vec<bool> = (0..plen).map(|i| pattern >> i & 1 == 1).collect();
    if bits.iter().all(|&b| b) || pattern == u32::MAX {
        return None;
    }
    if !bits.iter().any(|&b| b) {
        return Some(vec![0.0, plen as f64 / width.max(1.0)]);
    }
    // Rotate so the pattern starts with a run of ones.
    let start = (0..plen as usize).find(|&i| bits[i] && !bits[(i + plen as usize - 1) % plen as usize]).unwrap_or(0);
    let mut runs: Vec<f64> = Vec::new();
    let mut cur = true;
    let mut n = 0;
    for k in 0..plen as usize {
        let b = bits[(start + k) % plen as usize];
        if b == cur {
            n += 1;
        } else {
            runs.push(n as f64);
            cur = b;
            n = 1;
        }
    }
    runs.push(n as f64);
    if runs.len() % 2 == 1 {
        runs.push(0.0);
    }
    Some(runs.into_iter().map(|r| r / width.max(1.0)).collect())
}

struct Ctx<'a> {
    classes: &'a Classes,
    pens: Vec<PenDef>,
    labels: Vec<LabelStyle>,
    ramps: Vec<tables::Ramp>,
    fonts: BTreeMap<u8, String>,
    /// Classes present in the detail tileset (bands 8–9).
    detail_classes: BTreeSet<u16>,
    /// First zoom of the detail tileset.
    detail_from: f64,
    faces: BTreeSet<String>,
    /// Pixel classes of the current style's raster over water and land (draw-over masks, tints).
    surface: SurfaceClasses,
}

/// One compiled layer before merging: everything but the class filter.
#[derive(Clone)]
struct Proto {
    order: (u8, u32, u32),
    kind: &'static str,
    source: &'static str,
    source_layer: &'static str,
    minzoom: f64,
    maxzoom: f64,
    layout: Value,
    paint: Value,
    /// Combined with the class filter (draw-over masks: water or land runs only).
    filter: Option<Value>,
    placeholder: Option<String>,
}

/// Average advance of a label character in ems, for the abbreviation fallback (hypothesis: the
/// spec says only "when the full name does not fit", mad-styling.md §7.3).
const CHAR_EM: f64 = 0.6;

/// `text-field` of an area label: the name, or its abbreviation (U+0002) where the name is wider
/// than the area. The area's width at zoom 10 is the tile property `width_z10`; MapLibre allows
/// `zoom` only as the input of a top-level step, so the test is repeated at every half zoom.
fn area_text_field(size: f64, spacing_em: f64, z0: f64, z1: f64) -> Value {
    let name = json!(["coalesce", ["get", "name"], ""]);
    let fits = |z: f64| {
        let width = json!(["*", ["length", name.clone()], size * (CHAR_EM + spacing_em)]);
        json!(["case",
            ["all", ["has", "abbr"], ["has", "width_z10"], [">", width, ["*", ["get", "width_z10"], 2f64.powf(z - 10.0)]]],
            ["get", "abbr"], name.clone()])
    };
    let start = (z0 * 2.0).floor() / 2.0;
    let end = z1.min(18.0);
    let mut step = vec![json!("step"), json!(["zoom"]), fits(start)];
    let mut z = start + 0.5;
    while z < end {
        step.push(json!(z));
        step.push(fits(z));
        z += 0.5;
    }
    if step.len() == 3 { fits(start) } else { Value::Array(step) }
}

/// MapLibre filter for the line runs a pen may draw on (`None`: all of them).
fn side_filter(water: bool, land: bool) -> Option<Option<Value>> {
    match (water, land) {
        (true, true) => Some(None),
        (true, false) => Some(Some(json!(["==", ["get", "water"], true]))),
        (false, true) => Some(Some(json!(["!=", ["get", "water"], true]))),
        (false, false) => None,
    }
}

impl Ctx<'_> {
    fn tri(v: u8) -> u8 {
        v & 0x0F
    }

    /// Colour of a GeoTrans tint over this style's land: the ramp entry that the mean land
    /// brightness selects (mad-styling.md §6), so it comes from the light end over light rasters.
    fn tint(&self, ramp: u8) -> Option<[u8; 3]> {
        self.ramps.get(ramp as usize).map(|r| r[tables::tint_index(self.surface.land_luminance)])
    }

    fn pen_colour(&self, p: &PenDef) -> String {
        if p.b8[3] != 0xFF { self.tint(p.b8[3]).map_or(hex(p.color), hex) } else { hex(p.color) }
    }

    fn label_layout(&mut self, rule: &DisplayRule, kind: GeometryKind, zooms: (f64, f64)) -> Option<(Value, Value, Option<String>)> {
        let l = self.labels.get(rule.w5_13[0] as usize)?;
        let bold = l.tail[0] == 1;
        let italic = l.tail[1] == 1;
        let face = face_name(&self.fonts, l.b0, bold, italic);
        self.faces.insert(face.clone());
        let size = l.b4[0] as f64;
        let spacing = l.b4[6] as f64;
        let upper = l.nibbles.0 == 1;
        let mut layout = json!({
            "text-field": ["coalesce", ["get", "name"], ""],
            "text-font": [face],
            "text-size": size,
            "symbol-sort-key": ["get", "priority"],
        });
        if spacing > 0.0 {
            layout["text-letter-spacing"] = json!(spacing / size.max(1.0));
        }
        if upper {
            layout["text-transform"] = json!("uppercase");
        }
        match kind {
            GeometryKind::Line => {
                layout["symbol-placement"] = json!("line");
            }
            GeometryKind::Area => {
                layout["text-max-width"] = json!(8);
                layout["text-field"] = area_text_field(size, spacing / size.max(1.0), zooms.0, zooms.1);
            }
            _ => {
                layout["text-variable-anchor"] = json!(["left", "right", "top", "bottom"]);
                layout["text-radial-offset"] = json!(0.6);
            }
        }
        let paint = json!({
            "text-color": hex(l.color1),
            "text-halo-color": hex(rule.b18_20),
            "text-halo-width": 1,
            "text-halo-blur": 0,
        });
        let ph = (upper || spacing > 0.0).then(|| "GeoLabel byte 11 (upper case) and byte 10 (letter spacing) are hypotheses (mad-styling.md §7.2)".to_string());
        Some((layout, paint, ph))
    }

    /// Zoom range of a rule (styles are compiled for the reference pane at latitude 0).
    fn rule_zooms(rule: &DisplayRule) -> (f64, f64) {
        let z_lo = geo::zoom_for_altitude(rule.max_scale as f64 / 1000.0).max(0.0);
        let z_hi = geo::zoom_for_altitude((rule.min_scale as f64 / 1000.0).max(1.0)).min(24.0);
        (z_lo, z_hi)
    }

    /// Compiles one rule of one class into layer prototypes.
    fn compile(&mut self, rule: &DisplayRule, is_moon: bool) -> Vec<Proto> {
        let c = rule.class as usize;
        let class = &self.classes.classes[c];
        let kind = class.kind;
        let (z_lo, z_hi) = Self::rule_zooms(rule);
        if z_hi <= z_lo {
            return Vec::new();
        }
        let masked = !is_moon && self.classes.uses[c].masked > 0;
        let mut out = Vec::new();
        let z12 = class.group as u32;
        let prio = class.priority as u32;
        // Sources by zoom: the earth archive (overzoomed up to the detail zooms unless the class
        // has detail data).
        let detail_from = self.detail_from;
        let has_detail = self.detail_classes.contains(&rule.class);
        let sources: Vec<(&'static str, f64, f64)> = if is_moon {
            vec![("moon", z_lo, z_hi)]
        } else {
            let mut v = Vec::new();
            let earth_hi = if has_detail { z_hi.min(detail_from) } else { z_hi };
            if earth_hi > z_lo {
                v.push(("earth", z_lo, earth_hi));
            }
            if has_detail && z_hi > detail_from {
                v.push(("earth-detail", z_lo.max(detail_from), z_hi));
            }
            v
        };
        let geom = Self::tri(rule.b2);
        let label = rule.b3;
        let pens: Vec<Option<usize>> = rule.w5_13[2..9].iter().map(|&w| pen_ref(w)).collect();
        for &(src, lo, hi) in &sources {
            let base = |order: (u8, u32, u32), kind_s: &'static str, layer: &'static str, layout: Value, paint: Value, ph: Option<String>| Proto {
                order,
                kind: kind_s,
                source: src,
                source_layer: layer,
                minzoom: lo,
                maxzoom: hi,
                layout,
                paint,
                filter: None,
                placeholder: ph,
            };
            let vis = |v: u8| if v == 2 { json!({"visibility": "none"}) } else { json!({}) };
            if geom != 0 {
                match kind {
                    GeometryKind::Area => {
                        if let Some(p) = pens[0].and_then(|p| self.pens.get(p)) {
                            let mut paint = json!({ "fill-antialias": false });
                            let mut ph = None;
                            if p.w12 != NONE {
                                paint["fill-pattern"] = json!(format!("bm{}", p.w12));
                            } else if p.b8[3] != 0xFF {
                                paint["fill-color"] = json!(self.tint(p.b8[3]).map_or(hex(p.color), hex));
                                paint["fill-opacity"] = json!(0.5);
                                ph = Some(format!(
                                    "GeoTrans tint ramp {} over the hill shading is approximated by a 50 % fill of the ramp entry for the style's mean land brightness (mad-styling.md §6)",
                                    p.b8[3]
                                ));
                            } else {
                                paint["fill-color"] = json!(hex(p.color));
                            }
                            out.push(base((0, z12, prio), "fill", "areas", vis(geom), paint, ph));
                        }
                        // Outline pens stroke the seam-free `outlines` linework.
                        for (k, p) in pens.iter().enumerate().skip(1) {
                            if let Some(p) = p.and_then(|p| self.pens.get(p))
                                && p.b8[1] > 0
                            {
                                let paint = json!({ "line-color": self.pen_colour(p), "line-width": p.b8[1] });
                                let mut layout = vis(geom);
                                layout["line-join"] = json!("round");
                                out.push(base((1 + k as u8, z12, prio), "line", "outlines", layout, paint, None));
                            }
                        }
                    }
                    GeometryKind::Line => {
                        for (k, p) in pens.iter().enumerate() {
                            let Some(p) = p.and_then(|p| self.pens.get(p)) else { continue };
                            let (width, passes) = (p.b8[1] as f64, p.b8[2] as f64);
                            if width == 0.0 {
                                continue;
                            }
                            // Draw-over mask (mad-styling.md §5.2): which of the water and land runs.
                            let side = if masked {
                                let (w, l) = self.surface.pen_sides(pixel::pen_mask(&p.flags));
                                match side_filter(w, l) {
                                    Some(f) => f,
                                    None => continue,
                                }
                            } else {
                                None
                            };
                            let colour = self.pen_colour(p);
                            let offset = if p.signed24 != -128 { p.signed24 as f64 } else { 0.0 };
                            let strokes: Vec<(f64, f64)> = if passes >= width || passes == 0.0 {
                                vec![(width, offset)]
                            } else if passes == 1.0 {
                                vec![(1.0, offset + (width - 1.0) / 2.0)]
                            } else {
                                vec![(1.0, offset + (width - 1.0) / 2.0), (1.0, offset - (width - 1.0) / 2.0)]
                            };
                            for (w, off) in strokes {
                                let mut paint = json!({ "line-color": colour, "line-width": w });
                                if off != 0.0 {
                                    paint["line-offset"] = json!(off);
                                }
                                if let Some(d) = dasharray(p.u4, p.b8[0], w) {
                                    paint["line-dasharray"] = json!(d);
                                }
                                let mut layout = vis(geom);
                                layout["line-join"] = json!("round");
                                let mut proto = base((1 + k as u8, z12, prio), "line", "lines", layout, paint, None);
                                proto.filter = side.clone();
                                out.push(proto);
                            }
                        }
                    }
                    GeometryKind::Other(_) => {}
                    GeometryKind::Point | GeometryKind::RadiusPoint => {
                        if let Some(p) = pens[0].and_then(|p| self.pens.get(p)) {
                            if p.w12 != NONE {
                                let mut layout = vis(geom);
                                layout["icon-image"] = json!(format!("bm{}", p.w12));
                                layout["symbol-sort-key"] = json!(["get", "priority"]);
                                layout["icon-allow-overlap"] = json!(false);
                                if label != 0
                                    && let Some((tl, mut tp, ph)) = self.label_layout(rule, kind, (lo, hi))
                                {
                                    // Symbol and label appear together or not at all (§7.4); an
                                    // optional label on a shown symbol starts transparent.
                                    if label == 2 {
                                        tp["text-opacity"] = json!(0);
                                    }
                                    for (k, v) in tl.as_object().unwrap() {
                                        layout[k] = v.clone();
                                    }
                                    layout["icon-optional"] = json!(false);
                                    out.push(base((9, 0, prio), "symbol", "points", layout, tp, ph));
                                    continue;
                                }
                                out.push(base((9, 0, prio), "symbol", "points", layout, json!({}), None));
                                continue;
                            } else if p.b8[1] > 0 {
                                let paint = json!({ "circle-color": hex(p.color), "circle-radius": p.b8[1] as f64 / 2.0 });
                                out.push(base((8, z12, prio), "circle", "points", vis(geom), paint, None));
                            }
                        }
                    }
                }
            }
            if label != 0 && !(geom != 0 && matches!(kind, GeometryKind::Point | GeometryKind::RadiusPoint) && pens[0].and_then(|p| self.pens.get(p)).is_some_and(|p| p.w12 != NONE)) {
                let layer = match kind {
                    GeometryKind::Area => "labels",
                    GeometryKind::Line => "lines",
                    _ => "points",
                };
                if let Some((mut layout, paint, ph)) = self.label_layout(rule, kind, (lo, hi)) {
                    if label == 2 {
                        layout["visibility"] = json!("none");
                    }
                    out.push(base((10, 0, prio), "symbol", layer, layout, paint, ph));
                }
            }
            // Highlight of the selected place (b15/b16/w17). Encarta also names the mouse-over
            // highlight in code, but the VM shows none on hover (V11): the client draws these
            // for its selection, and on hover only as an option.
            if rule.b14_16[1] == 1
                && let Some(p) = pen_ref(rule.w17).and_then(|p| self.pens.get(p))
            {
                let colour = self.pen_colour(p);
                let on = json!(["case", ["boolean", ["feature-state", "hover"], false], 1, 0]);
                if rule.b14_16[2] == 3 && kind == GeometryKind::Area {
                    let ph = (p.b8[3] != 0xFF).then(|| format!("highlight tint ramp {}: one ramp entry (for the style's mean land brightness) at 60 %, not per pixel (mad-styling.md §6)", p.b8[3]));
                    out.push(base((7, z12, prio), "fill", "areas", json!({}), json!({ "fill-color": colour, "fill-opacity": ["*", 0.6, on] }), ph));
                } else if kind != GeometryKind::Point && kind != GeometryKind::RadiusPoint {
                    let layer = if kind == GeometryKind::Area { "outlines" } else { "lines" };
                    let w = p.b8[1].max(1);
                    out.push(base((7, z12, prio), "line", layer, json!({}), json!({ "line-color": colour, "line-width": w, "line-opacity": on }), None));
                }
            }
        }
        out
    }
}

/// Per class, the zoom ranges in which the rules make it interactive (PDCAltitudeGrid `b4` low
/// nibble, mad-styling.md §4.1): `[[minzoom, maxzoom, tri-state], …]`, adjacent equal ranges
/// merged, tri-state 1 = by default, 2 = when the user turns the class on.
fn interactive_ranges(rules: &[DisplayRule]) -> BTreeMap<u16, Vec<(f64, f64, u8)>> {
    let mut out: BTreeMap<u16, Vec<(f64, f64, u8)>> = BTreeMap::new();
    for r in rules {
        let tri = r.b4 & 0x0F;
        let (lo, hi) = Ctx::rule_zooms(r);
        if tri == 0 || hi <= lo {
            continue;
        }
        let v = out.entry(r.class).or_default();
        let (lo, hi) = ((lo * 100.0).floor() / 100.0, (hi * 100.0).ceil() / 100.0);
        if let Some(last) = v.iter_mut().find(|e| e.2 == tri && ((e.1 - lo).abs() < 0.02 || (hi - e.0).abs() < 0.02)) {
            last.0 = last.0.min(lo);
            last.1 = last.1.max(hi);
        } else {
            v.push((lo, hi, tri));
        }
    }
    for v in out.values_mut() {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    out
}

#[derive(Debug, Default, Serialize)]
pub struct StyleReport {
    pub styles: Vec<(u16, String, String, usize)>,
    pub rules: usize,
    pub layers: usize,
    pub placeholder_layers: usize,
    /// `(style id, water pixel class, land pixel class, mean land luminance)` of each style's raster.
    pub surfaces: Vec<(u16, u8, u8, f64)>,
    pub faces: Vec<String>,
    pub sprites: usize,
}

/// A vector source. `minzoom`/`maxzoom` repeat the archive's own (the PMTiles header wins in
/// MapLibre); the client's flat view needs them when it re-tiles the archive itself.
fn tileset_source(ts: Tileset, cells: &CellSystem) -> Value {
    let zb = vector::zoom_bands(cells, ts);
    let (lo, hi) = (zb.first().map_or(0, |z| z.0), zb.last().map_or(0, |z| z.0));
    json!({ "type": "vector", "url": format!("pmtiles:///atlas/{}", ts.path()), "minzoom": lo, "maxzoom": hi, "attribution": "Microsoft Encarta 2009" })
}

pub fn run(atlas: &Atlas, classes: &Classes, detail_classes: BTreeSet<u16>, out: &Path, log: &dyn Fn(String)) -> Result<(StyleReport, BTreeSet<String>)> {
    let mut rep = StyleReport::default();
    let cells = atlas.cells()?;
    let fonts: BTreeMap<u8, String> =
        tables::label_fonts(atlas.require("Label Font")?)?.into_iter().filter(|f| f.charset == tables::CHARSET_ANSI).map(|f| (f.font, f.name)).collect();
    let mut ctx = Ctx {
        classes,
        pens: tables::pen_defs(atlas.require("GeoPenDef")?)?,
        labels: tables::label_styles(atlas.require("GeoLabel")?)?,
        ramps: tables::geo_trans(atlas.require("GeoTrans")?)?,
        fonts,
        detail_classes,
        detail_from: vector::detail_from_zoom(&cells) as f64,
        faces: BTreeSet::new(),
        surface: SurfaceClasses { water: 1, land: 4, land_luminance: 128.0 },
    };
    let terrain = atlas.terrain()?;
    let hypso = terrain.raster("HYPSO").context("no HYPSO raster")?;
    let rules_grid = atlas.grid("PDCAltitudeGrid", "PDCAltitudeLookup")?;
    let min_rects = tables::min_rects(atlas.require("MinRect")?)?;
    let dir = out.join("styles");
    std::fs::create_dir_all(&dir)?;
    let mut index = Vec::new();
    for s in atlas.map_styles()? {
        let info = StyleInfo { style: s };
        let s = &info.style;
        let rules: DisplayRules = decode_record(&rules_grid, s.id as u32).with_context(|| format!("rules of style {}", s.id))?;
        rep.rules += rules.rules.len();
        let is_moon_style = s.raster.eq_ignore_ascii_case("MOON");
        let raster = terrain.raster(&s.raster).with_context(|| format!("raster {} of style {}", s.raster, s.name))?;
        let look = raster.look(&s.look).with_context(|| format!("LOOK {} of style {}", s.look, s.name))?;
        ctx.surface = pixel::calibrate(hypso, raster, look, 4096)?;
        rep.surfaces.push((s.id, ctx.surface.water, ctx.surface.land, (ctx.surface.land_luminance * 10.0).round() / 10.0));
        let mut protos: Vec<(Proto, u16)> = Vec::new();
        for r in &rules.rules {
            let moon = classes.is_moon(r.class as usize);
            if moon != is_moon_style {
                continue;
            }
            for p in ctx.compile(r, moon) {
                protos.push((p, r.class));
            }
        }
        // Merge identical layers.
        let mut merged: BTreeMap<String, (Proto, Vec<u16>)> = BTreeMap::new();
        for (p, c) in protos {
            let key = serde_json::to_string(&json!([p.order.0, p.order.1, p.kind, p.source, p.source_layer,
                (p.minzoom * 100.0).round(), (p.maxzoom * 100.0).round(), p.layout, p.paint, p.filter]))?;
            let e = merged.entry(key).or_insert_with(|| (p.clone(), Vec::new()));
            e.0.order.2 = e.0.order.2.min(p.order.2);
            if !e.1.contains(&c) {
                e.1.push(c);
            }
        }
        let mut groups: Vec<(Proto, Vec<u16>)> = merged.into_values().collect();
        groups.sort_by(|a, b| a.0.order.cmp(&b.0.order).then(a.0.minzoom.total_cmp(&b.0.minzoom)));
        let raster = s.raster.to_ascii_lowercase();
        let look = s.look.to_ascii_lowercase();
        let mut sources = serde_json::Map::new();
        sources.insert("raster".into(), json!({ "type": "raster", "url": format!("pmtiles:///atlas/raster/{raster}/{look}.pmtiles"), "tileSize": 256 }));
        sources.insert("raster-index".into(), json!({ "type": "raster", "url": format!("pmtiles:///atlas/raster/{raster}/index.pmtiles"), "tileSize": 256 }));
        if is_moon_style {
            sources.insert("moon".into(), tileset_source(Tileset::Moon, &cells));
        } else {
            sources.insert("earth".into(), tileset_source(Tileset::Earth, &cells));
            sources.insert("earth-detail".into(), tileset_source(Tileset::EarthDetail, &cells));
        }
        let mut layers = vec![
            json!({ "id": "background", "type": "background", "paint": { "background-color": hex(info.globe()) } }),
            json!({ "id": "raster", "type": "raster", "source": "raster", "paint": { "raster-resampling": "nearest" } }),
        ];
        let mut placeholders = 0;
        for (i, (p, cls)) in groups.iter().enumerate() {
            let class_filter = if cls.len() == 1 { json!(["==", ["get", "class"], cls[0]]) } else { json!(["in", ["get", "class"], ["literal", cls]]) };
            // The class test stays first: the client rewrites it for the user's class overrides.
            let filter = match &p.filter {
                Some(f) => json!(["all", class_filter, f]),
                None => class_filter,
            };
            let mut l = json!({
                "id": format!("{}-{}-{i}", p.kind, p.source_layer),
                "type": p.kind, "source": p.source, "source-layer": p.source_layer,
                "minzoom": (p.minzoom * 100.0).floor() / 100.0, "maxzoom": (p.maxzoom * 100.0).ceil() / 100.0,
                "filter": filter, "layout": p.layout, "paint": p.paint,
                "metadata": { "encarta:classes": cls.iter().map(|&c| classes.classes[c as usize].name.clone()).collect::<Vec<_>>() },
            });
            if let Some(ph) = &p.placeholder {
                l["metadata"]["encarta:placeholder"] = json!(ph);
                placeholders += 1;
            }
            layers.push(l);
        }
        let interactive: serde_json::Map<String, Value> = interactive_ranges(&rules.rules)
            .into_iter()
            .filter(|(c, _)| classes.is_moon(*c as usize) == is_moon_style)
            .map(|(c, v)| (c.to_string(), json!(v.iter().map(|r| json!([r.0, r.1, r.2])).collect::<Vec<_>>())))
            .collect();
        let rects: Vec<Value> = min_rects
            .iter()
            .filter(|r| r.style == s.id)
            .map(|r| json!({ "bbox": r.bbox(), "min_altitude_km": r.altitude_m as f64 / 1000.0, "maxzoom": geo::zoom_for_altitude(r.altitude_m as f64 / 1000.0) }))
            .collect();
        let style = json!({
            "version": 8,
            "name": s.name,
            "projection": { "type": "globe" },
            "glyphs": "/atlas/glyphs/{fontstack}/{range}.pbf",
            "sprite": "/atlas/sprites/sprite",
            "sources": sources,
            "layers": layers,
            "metadata": {
                "encarta:style_id": s.id,
                "encarta:raster": s.raster, "encarta:look": s.look,
                "encarta:space_colour": hex(info.space()),
                "encarta:default_style": info.style.is_default,
                "encarta:min_altitude_km": info.style.min_altitude_m as f64 / 1000.0,
                "encarta:max_altitude_km": info.style.max_altitude_m as f64 / 1000.0,
                "encarta:minzoom": geo::zoom_for_altitude(info.style.max_altitude_m as f64 / 1000.0).max(0.0),
                "encarta:maxzoom": geo::zoom_for_altitude(info.style.min_altitude_m as f64 / 1000.0),
                "encarta:min_rects": rects,
                "encarta:view_p_over_m": geo::VIEW_P_OVER_M,
                "encarta:view_p_over_m_per_height_px": geo::VIEW_P_OVER_M_PER_HEIGHT_PX,
                "encarta:reference_pane_height_px": geo::REFERENCE_PANE_HEIGHT_PX,
                "encarta:interactive": interactive,
                "encarta:surface": { "water_pixel_class": ctx.surface.water, "land_pixel_class": ctx.surface.land, "land_luminance": (ctx.surface.land_luminance * 10.0).round() / 10.0 },
                "encarta:notes": "Zoom ranges come from Encarta camera altitudes via zoom = log2(40075·(P/m)/(1280·alt_km)) at latitude 0 for Encarta's 812 × 518 map pane (P/m = 3135, measured in the VM; P/m ≈ 6.05 × pane height). A client shifts the zoom ranges by log2(cos φ · P/m(own pane) / 3135) to apply Encarta's rule at the true ground scale. The raster-index source carries class indices for client-side recolouring (raster/palettes.json).",
            },
        });
        let name = format!("{}.json", info.slug());
        std::fs::write(dir.join(&name), serde_json::to_vec_pretty(&style)?)?;
        rep.layers += layers.len();
        rep.placeholder_layers += placeholders;
        rep.styles.push((s.id, s.name.clone(), format!("styles/{name}"), layers.len()));
        index.push(json!({ "id": s.id, "name": s.name, "file": format!("styles/{name}"), "raster": s.raster, "look": s.look, "default": info.style.is_default }));
    }
    std::fs::write(dir.join("index.json"), serde_json::to_vec_pretty(&index)?)?;
    rep.sprites = sprites(atlas, &out.join("sprites"))?;
    rep.faces = ctx.faces.iter().cloned().collect();
    log(format!("  {} styles, {} layers ({} with placeholders), {} sprites, faces {:?}", rep.styles.len(), rep.layers, rep.placeholder_layers, rep.sprites, rep.faces));
    Ok((rep, ctx.faces))
}

/// Bitmap symbols → `sprite.png/json` and `sprite@2x.png/json`.
fn sprites(atlas: &Atlas, dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(dir)?;
    let grid = atlas.grid("Bitmap", "BitmapLookup")?;
    let palette = atlas.bitmap_palette()?;
    let mut icons: Vec<(u32, usize, usize, Vec<u8>)> = Vec::new();
    for run in grid.index.runs() {
        for key in run.first..=run.last() {
            let b: Bitmap = decode_record(&grid, key)?;
            icons.push((key - 0x10000, b.width as usize, b.height as usize, b.to_rgba(&palette)));
        }
    }
    for ratio in [1usize, 2] {
        // Shelf packing in rows of at most 512 px.
        let (mut x, mut y, mut row_h, mut w_max) = (0usize, 0usize, 0usize, 0usize);
        let mut place = Vec::new();
        for (_, w, h, _) in &icons {
            let (w, h) = (w * ratio, h * ratio);
            if x + w > 512 {
                x = 0;
                y += row_h + 1;
                row_h = 0;
            }
            place.push((x, y));
            x += w + 1;
            row_h = row_h.max(h);
            w_max = w_max.max(x);
        }
        let (sw, sh) = (w_max.max(1), y + row_h.max(1));
        let mut img = vec![0u8; sw * sh * 4];
        let mut index = serde_json::Map::new();
        for ((id, w, h, rgba), &(px, py)) in icons.iter().zip(&place) {
            for yy in 0..h * ratio {
                for xx in 0..w * ratio {
                    let s = ((yy / ratio) * w + xx / ratio) * 4;
                    let d = ((py + yy) * sw + px + xx) * 4;
                    img[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                }
            }
            index.insert(format!("bm{id}"), json!({ "x": px, "y": py, "width": w * ratio, "height": h * ratio, "pixelRatio": ratio }));
        }
        let stem = if ratio == 1 { "sprite".to_string() } else { format!("sprite@{ratio}x") };
        let mut png_bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_bytes, sw as u32, sh as u32);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()?.write_image_data(&img)?;
        }
        std::fs::write(dir.join(format!("{stem}.png")), png_bytes)?;
        std::fs::write(dir.join(format!("{stem}.json")), serde_json::to_vec_pretty(&Value::Object(index))?)?;
    }
    Ok(icons.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_patterns() {
        assert_eq!(dasharray(u32::MAX, 32, 1.0), None);
        // 0x7, plen 5: three on, two off (Ferry).
        assert_eq!(dasharray(0x7, 5, 1.0), Some(vec![3.0, 2.0]));
        // 0x3F, plen 10, width 2: six on, four off → in line widths.
        assert_eq!(dasharray(0x3F, 10, 2.0), Some(vec![3.0, 2.0]));
        // Pattern not starting at bit 0 is rotated to start with ones.
        assert_eq!(dasharray(0b0110, 4, 1.0), Some(vec![2.0, 2.0]));
    }

    #[test]
    fn names() {
        assert_eq!(slug("Atlas Default-No HS"), "atlas-default-no-hs");
        let fonts = BTreeMap::from([(9u8, "MS Reference Sans Serif".to_string())]);
        assert_eq!(face_name(&fonts, 9, true, false), "MS Reference Sans Serif Bold");
    }
}
