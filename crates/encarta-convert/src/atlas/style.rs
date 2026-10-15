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
use super::shield::{self, Shield};
use super::vector;
use super::vector::Tileset;
use anyhow::{Context, Result};
use encarta_formats::mad::Atlas;
use encarta_formats::mad::cells::CellSystem;
use encarta_formats::mad::schemas::{Bitmap, DisplayRule, DisplayRules, decode_record};
use encarta_formats::mad::tables::{self, FeatureClass, GeometryKind, LabelStyle, MapStyle, PenDef};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// PDCAttribute pen group whose classes the click menu leaves out.
const MENU_EXCLUDED_PEN_GROUP: u16 = 134;
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
    /// Shield sprites the styles refer to (mad-styling.md §8.1).
    shields: BTreeSet<Shield>,
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

/// The alternative-name line of a label (mad-styling.md §4.1 `w6`, §7.3): MOBB draws it as an extra last
/// line under the main name, in the `w6` style, with a 1-px rule between the lines as wide as the wider line.
struct AltLine {
    /// Size relative to the main name's.
    scale: f64,
    colour: String,
    upper: bool,
    /// Glyph faces of the two lines with the rule built in (`rule_faces`).
    main_rule_face: String,
    alt_rule_face: String,
}

/// MapLibre's default `text-line-height` (ems), which the label layers keep.
const LINE_HEIGHT_EM: f64 = 1.2;
/// Rule thickness in glyph units (24 per em): about 1 px at the 11–14 px label sizes.
const RULE_THICKNESS: f64 = 2.0;

/// Glyph faces for the rule between a name and its alternative (mad-styling.md §7.3). MapLibre has no
/// text decoration, so each glyph of the main line carries an underline and each glyph of the alternative
/// an overline, both at the main line's descender: the union spans the wider line, aligned like the text.
/// Glyph frames are 24 units per em with the line top at 0 and the descender at 24 (glyphs.rs scales every
/// face so), and the alternative's line starts `LINE_HEIGHT_EM` main ems lower. `spacing`: the letter
/// spacing in glyph units, split over both sides of each glyph so that the rule has no gaps.
/// Face names: `"<face> Rule <top> <height> <overhang>"` (glyph units of that line's scale).
fn rule_faces(main_face: &str, alt_face: &str, alt_scale: f64, spacing: f64) -> (String, String) {
    let k = alt_scale.max(0.1);
    let top = 24.0 - LINE_HEIGHT_EM * 24.0;
    (
        format!("{main_face} Rule 24.00 {:.2} {:.2}", RULE_THICKNESS, spacing / 2.0),
        format!("{alt_face} Rule {:.2} {:.2} {:.2}", top / k, RULE_THICKNESS / k, spacing / 2.0 / k),
    )
}

/// Text of a label: the name (upper case by GeoLabel byte 26) and, with an alternative style, the
/// alternative as a last line.
struct TextSpec {
    upper: bool,
    alt: Option<AltLine>,
}

impl TextSpec {
    fn name(&self) -> Value {
        let n = json!(["coalesce", ["get", "name"], ""]);
        if self.upper { json!(["upcase", n]) } else { n }
    }

    fn abbr(&self) -> Value {
        let a = json!(["get", "abbr"]);
        if self.upper { json!(["upcase", a]) } else { a }
    }

    /// `main` plus the alternative line where the feature has one. Brackets only when the main name starts
    /// with kana or a CJK ideograph (U+3040…U+9FFF).
    fn with_alt(&self, main: Value) -> Value {
        let Some(alt) = &self.alt else { return main };
        let first = json!(["slice", ["coalesce", ["get", "name"], ""], 0, 1]);
        let cjk = json!(["all", [">=", first.clone(), "\u{3040}"], ["<=", first, "\u{9fff}"]]);
        let a = json!(["get", "alt_names"]);
        let a = if alt.upper { json!(["upcase", a]) } else { a };
        let a = json!(["case", cjk, ["concat", "(", a.clone(), ")"], a]);
        json!(["case", ["has", "alt_names"],
            ["format", main.clone(), { "text-font": ["literal", [alt.main_rule_face.clone()]] }, "\n", {},
                a, { "text-font": ["literal", [alt.alt_rule_face.clone()]], "font-scale": alt.scale, "text-color": alt.colour }],
            main])
    }

    /// `text-max-width` (ems): a name with an alternative stays on one line, so that the rule is drawn
    /// only once (Encarta may still wrap it: "Caldaro sulla strada del vino" in two lines, V15).
    fn max_width(&self, ems: f64) -> Value {
        if self.alt.is_some() { json!(["case", ["has", "alt_names"], 1000, ems]) } else { json!(ems) }
    }
}

/// `text-field` of an area label: the name, or its abbreviation (U+0002) where the name is wider
/// than the area. The area's width at zoom 10 is the tile property `width_z10`; MapLibre allows
/// `zoom` only as the input of a top-level step, so the test is repeated at every half zoom.
fn area_text_field(size: f64, spacing_em: f64, z0: f64, z1: f64, text: &TextSpec) -> Value {
    let name = json!(["coalesce", ["get", "name"], ""]);
    let fits = |z: f64| {
        let width = json!(["*", ["length", name.clone()], size * (CHAR_EM + spacing_em)]);
        json!(["case",
            ["all", ["has", "abbr"], ["has", "width_z10"], [">", width, ["*", ["get", "width_z10"], 2f64.powf(z - 10.0)]]],
            text.abbr(), text.with_alt(text.name())])
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

/// MapLibre filter for the line runs or polygon parts a pen may draw on (`None`: all of them).
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

    fn label_layout(&mut self, rule: &DisplayRule, kind: GeometryKind, zooms: (f64, f64), symbol: bool) -> Option<(Value, Value, Option<String>)> {
        let l = self.labels.get(rule.w5_13[0] as usize)?.clone();
        let bold = l.tail[0] == 1;
        let italic = l.tail[1] == 1;
        let face = face_name(&self.fonts, l.b0, bold, italic);
        self.faces.insert(face.clone());
        let size = l.b4[0] as f64;
        let spacing = l.b4[6] as f64;
        // The alternative name (w6), under the main name; not on lines, whose text follows the line.
        let alt = self.labels.get(rule.w5_13[1] as usize).cloned().filter(|_| kind != GeometryKind::Line).map(|l6| {
            let alt_face = face_name(&self.fonts, l6.b0, l6.tail[0] == 1, l6.tail[1] == 1);
            let scale = l6.b4[0] as f64 / size.max(1.0);
            let (main_rule_face, alt_rule_face) = rule_faces(&face, &alt_face, scale, spacing / size.max(1.0) * 24.0);
            self.faces.extend([main_rule_face.clone(), alt_rule_face.clone()]);
            AltLine { scale, colour: hex(l6.color1), upper: l6.tail[3] == 1, main_rule_face, alt_rule_face }
        });
        let text = TextSpec { upper: l.tail[3] == 1, alt };
        let mut layout = json!({
            "text-field": text.with_alt(text.name()),
            "text-font": [face],
            "text-size": size,
            "symbol-sort-key": ["get", "priority"],
        });
        if spacing > 0.0 {
            layout["text-letter-spacing"] = json!(spacing / size.max(1.0));
        }
        match kind {
            GeometryKind::Line => {
                layout["symbol-placement"] = json!("line");
            }
            GeometryKind::Area => {
                layout["text-max-width"] = text.max_width(8.0);
                layout["text-field"] = area_text_field(size, spacing / size.max(1.0), zooms.0, zooms.1, &text);
            }
            _ => {
                if text.alt.is_some() {
                    layout["text-max-width"] = text.max_width(10.0);
                }
                layout["text-variable-anchor"] = point_label_anchors(symbol);
                // Lines align on the symbol's side ("Mumbai" over "Bombay" right-aligned left of
                // the city, V15).
                layout["text-justify"] = json!("auto");
                layout["text-radial-offset"] = json!(if symbol { 0.3 } else { 0.0 });
            }
        }
        let paint = json!({
            "text-color": hex(l.color1),
            "text-halo-color": hex(rule.b18_20),
            "text-halo-width": 1,
            "text-halo-blur": 0,
        });
        let ph = (spacing > 0.0).then(|| "GeoLabel byte 10 (letter spacing) is a hypothesis (mad-styling.md §7.2)".to_string());
        Some((layout, paint, ph))
    }

    /// Road-shield layer of a line rule whose `w5` label has a shield shape (mad-styling.md §8.1):
    /// the road's name upright on a stretched shield sprite, repeated rarely along the road.
    fn shield_layout(&mut self, rule: &DisplayRule) -> Option<(Value, Value)> {
        let l = self.labels.get(rule.w5_13[0] as usize)?.clone();
        if l.b12 == 0 {
            return None;
        }
        let face = face_name(&self.fonts, l.b0, l.tail[0] == 1, l.tail[1] == 1);
        self.faces.insert(face.clone());
        let sh = Shield { shape: l.b12, outline: l.color2, width: l.b16, fill: l.color3, fill2: l.color4 };
        let name = json!(["coalesce", ["get", "name"], ""]);
        let layout = json!({
            "symbol-placement": "line",
            // Encarta puts at most one shield on each on-screen run, near its middle, and drops it when
            // one with the same text is within ~160 px (812 × 518 pane; mad-styling.md §8.1). MapLibre
            // repeats line labels at the spacing and drops a repeat of the same text within half of it.
            "symbol-spacing": 400,
            "symbol-sort-key": ["get", "priority"],
            "text-field": if l.tail[3] == 1 { json!(["upcase", name]) } else { name },
            "text-font": [face],
            "text-size": l.b4[0] as f64,
            "text-rotation-alignment": "viewport",
            "text-pitch-alignment": "viewport",
            "text-padding": 2,
            "text-allow-overlap": false,
            "icon-image": sh.name(),
            "icon-text-fit": "both",
            "icon-text-fit-padding": [1, 3, 1, 3],
            "icon-rotation-alignment": "viewport",
            "icon-pitch-alignment": "viewport",
            "icon-padding": 2,
            "icon-allow-overlap": false,
        });
        self.shields.insert(sh);
        Some((layout, json!({ "text-color": hex(l.color1) })))
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
        let masked_fill = !is_moon && self.classes.uses[c].masked_fill > 0;
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
                        // Draw-over mask of the fill (mad-styling.md §5.2): which of the water and
                        // land parts; a fill that may draw on neither is dropped.
                        let fill = pens[0].and_then(|p| self.pens.get(p)).and_then(|p| {
                            if !masked_fill {
                                return Some((p, None));
                            }
                            let (w, l) = self.surface.pen_sides(pixel::pen_mask(&p.flags));
                            side_filter(w, l).map(|f| (p, f))
                        });
                        if let Some((p, side)) = fill {
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
                            let mut proto = base((0, z12, prio), "fill", "areas", vis(geom), paint, ph);
                            proto.filter = side;
                            out.push(proto);
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
                                    && let Some((tl, mut tp, ph)) = self.label_layout(rule, kind, (lo, hi), true)
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
            if label != 0
                && kind == GeometryKind::Line
                && let Some((layout, paint)) = self.shield_layout(rule)
            {
                let mut layout = layout;
                if label == 2 {
                    layout["visibility"] = json!("none");
                }
                let mut proto = base((11, 0, prio), "symbol", "lines", layout, paint, None);
                proto.filter = Some(json!(["has", "name"]));
                out.push(proto);
            } else if label != 0 && !(geom != 0 && matches!(kind, GeometryKind::Point | GeometryKind::RadiusPoint) && pens[0].and_then(|p| self.pens.get(p)).is_some_and(|p| p.w12 != NONE)) {
                let layer = match kind {
                    GeometryKind::Area => "labels",
                    GeometryKind::Line => "lines",
                    _ => "points",
                };
                if let Some((mut layout, paint, ph)) = self.label_layout(rule, kind, (lo, hi), false) {
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

/// Where a layer goes in the stack. MapLibre places symbols from the top layer down, so the
/// first-placed (winning) labels must be on top: symbol layers are stacked by descending
/// `PDCAttribute.b13`, so that the lowest value (mad-styling.md §7.4: lower wins) is placed first.
/// Points, area labels and line labels share one collision pass in MOBB, so they interleave.
fn stack_key(p: &Proto) -> (u8, u32, u32) {
    if p.kind == "symbol" {
        (9, u32::MAX - p.order.2, u32::from(p.order.0))
    } else {
        p.order
    }
}

/// Per class, the zoom ranges in which the rules make it interactive (PDCAltitudeGrid `b4`,
/// mad-styling.md §4.1, §4.3, §4.4): `[[minzoom, maxzoom, tri-state, hit side], …]`, adjacent equal ranges
/// merged. Tri-state (low nibble) 1 = by default, 2 = when the user turns the class on. Hit side
/// (high nibble) 0 = anywhere, 1 = land (left out when a water hit is listed too), 2 = water
/// (listed only over water); 0 in Moon styles, whose polygons are not split.
/// MapLibre anchors of a point label, in MOBB's candidate order (mad-styling.md §7.4): the text's
/// corner at the symbol's corner, above right, below right, above left, below left. MOBB adds 250,
/// 500, 750 or 1000 to each candidate's overlap cost and keeps the cheapest, so with nothing in the
/// way the first free corner wins; MapLibre takes the first corner that does not collide. A label
/// without a drawn symbol is centred on its point first (no added cost), then tries the corners.
fn point_label_anchors(symbol: bool) -> Value {
    let corners = ["bottom-left", "top-left", "bottom-right", "top-right"];
    if symbol { json!(corners) } else { json!(std::iter::once("center").chain(corners).collect::<Vec<_>>()) }
}

/// Rows of the classes that ENCARTA.EXE never puts in the click menu: PDCAttribute pen group 134
/// (time zones, DST areas, "Shading - gray"; mad-styling.md §16 V11 result). Rows, not class ids:
/// the tiles' `class` and the hit list are by row.
fn menu_excluded_rows(classes: &[FeatureClass], is_moon: impl Fn(usize) -> bool, moon: bool) -> Vec<u16> {
    classes.iter().enumerate().filter(|&(row, c)| c.pen_group == MENU_EXCLUDED_PEN_GROUP && is_moon(row) == moon).map(|(row, _)| row as u16).collect()
}

/// What the click menu sorts by besides the kind of hit (mad-styling.md §4.5), for the classes in
/// the hit list: PDCAttribute scale / 1000 by row, and the rows with the routable-road flag (byte 6).
fn class_hit_order(classes: &[FeatureClass], rows: impl Iterator<Item = u16>) -> (serde_json::Map<String, Value>, Vec<u16>) {
    let mut scale = serde_json::Map::new();
    let mut routable = Vec::new();
    for row in rows {
        let Some(c) = classes.get(row as usize) else { continue };
        scale.insert(row.to_string(), json!(c.scale / 1000));
        if c.flag != 0 {
            routable.push(row);
        }
    }
    (scale, routable)
}

fn interactive_ranges(rules: &[DisplayRule], moon: bool) -> BTreeMap<u16, Vec<(f64, f64, u8, u8)>> {
    let mut out: BTreeMap<u16, Vec<(f64, f64, u8, u8)>> = BTreeMap::new();
    for r in rules {
        let tri = r.b4 & 0x0F;
        let side = if moon { 0 } else { r.b4 >> 4 };
        let (lo, hi) = Ctx::rule_zooms(r);
        if tri == 0 || hi <= lo {
            continue;
        }
        let v = out.entry(r.class).or_default();
        let (lo, hi) = ((lo * 100.0).floor() / 100.0, (hi * 100.0).ceil() / 100.0);
        if let Some(last) = v.iter_mut().find(|e| e.2 == tri && e.3 == side && ((e.1 - lo).abs() < 0.02 || (hi - e.0).abs() < 0.02)) {
            last.0 = last.0.min(lo);
            last.1 = last.1.max(hi);
        } else {
            v.push((lo, hi, tri, side));
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
        shields: BTreeSet::new(),
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
        groups.sort_by(|a, b| stack_key(&a.0).cmp(&stack_key(&b.0)).then(a.0.minzoom.total_cmp(&b.0.minzoom)));
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
        let interactive: serde_json::Map<String, Value> = interactive_ranges(&rules.rules, is_moon_style)
            .into_iter()
            .filter(|(c, _)| classes.is_moon(*c as usize) == is_moon_style)
            .map(|(c, v)| (c.to_string(), json!(v.iter().map(|r| json!([r.0, r.1, r.2, r.3])).collect::<Vec<_>>())))
            .collect();
        // Classes of PDCAttribute pen group 134 (time zones, DST areas, "Shading - gray") never enter the click
        // menu (mad-styling.md §16 V11 result).
        let menu_excluded = menu_excluded_rows(&classes.classes, |row| classes.is_moon(row), is_moon_style);
        let (class_scale, routable) = class_hit_order(&classes.classes, interactive.keys().filter_map(|k| k.parse().ok()));
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
                "encarta:menu_excluded_classes": menu_excluded,
                "encarta:class_scale": class_scale,
                "encarta:routable_classes": routable,
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
    rep.sprites = sprites(atlas, &out.join("sprites"), &ctx.shields)?;
    rep.faces = ctx.faces.iter().cloned().collect();
    log(format!("  {} styles, {} layers ({} with placeholders), {} sprites, faces {:?}", rep.styles.len(), rep.layers, rep.placeholder_layers, rep.sprites, rep.faces));
    Ok((rep, ctx.faces))
}

/// Bitmap symbols → `sprite.png/json` and `sprite@2x.png/json`.
fn sprites(atlas: &Atlas, dir: &Path, shields: &BTreeSet<Shield>) -> Result<usize> {
    std::fs::create_dir_all(dir)?;
    let grid = atlas.grid("Bitmap", "BitmapLookup")?;
    let palette = atlas.bitmap_palette()?;
    let mut icons: Vec<(String, usize, usize, Vec<u8>)> = Vec::new();
    for run in grid.index.runs() {
        for key in run.first..=run.last() {
            let b: Bitmap = decode_record(&grid, key)?;
            icons.push((format!("bm{}", key - 0x10000), b.width as usize, b.height as usize, b.to_rgba(&palette)));
        }
    }
    let bitmaps = icons.len();
    for ratio in [1usize, 2] {
        // Sprites at this ratio: `(name, width, height, rgba, extra sprite JSON fields)`. Bitmaps are
        // pixel-doubled; shields are drawn at the ratio.
        let mut items: Vec<(String, usize, usize, Vec<u8>, Value)> = Vec::new();
        for (name, w, h, rgba) in &icons {
            let (pw, ph) = (w * ratio, h * ratio);
            let mut big = vec![0u8; pw * ph * 4];
            for yy in 0..ph {
                for xx in 0..pw {
                    let s = ((yy / ratio) * w + xx / ratio) * 4;
                    let d = (yy * pw + xx) * 4;
                    big[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                }
            }
            items.push((name.clone(), pw, ph, big, json!({})));
        }
        for sh in shields {
            let (n, rgba, meta) = shield::render(sh, ratio);
            items.push((sh.name(), n, n, rgba, meta));
        }
        // Shelf packing in rows of at most 512 px.
        let (mut x, mut y, mut row_h, mut w_max) = (0usize, 0usize, 0usize, 0usize);
        let mut place = Vec::new();
        for (_, w, h, _, _) in &items {
            if x + w > 512 {
                x = 0;
                y += row_h + 1;
                row_h = 0;
            }
            place.push((x, y));
            x += w + 1;
            row_h = row_h.max(*h);
            w_max = w_max.max(x);
        }
        let (sw, sh) = (w_max.max(1), y + row_h.max(1));
        let mut img = vec![0u8; sw * sh * 4];
        let mut index = serde_json::Map::new();
        for ((name, w, h, rgba, meta), &(px, py)) in items.iter().zip(&place) {
            for yy in 0..*h {
                let d = ((py + yy) * sw + px) * 4;
                img[d..d + w * 4].copy_from_slice(&rgba[yy * w * 4..(yy + 1) * w * 4]);
            }
            let mut e = json!({ "x": px, "y": py, "width": w, "height": h, "pixelRatio": ratio });
            if let Some(m) = meta.as_object() {
                e.as_object_mut().unwrap().extend(m.clone());
            }
            index.insert(name.clone(), e);
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
    Ok(bitmaps + shields.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_ranges_carry_the_hit_side() {
        let rule = |class: u16, b4: u8, max_km: u32, min_km: u32| DisplayRule {
            class, b2: 16, b3: 0, b4, w5_13: [0xFFFF; 9], b14_16: [0; 3], w17: 0xFFFF, b18_20: [0; 3], w21_24: [0xFFFF; 4],
            max_scale: max_km * 1000, min_scale: min_km * 1000,
        };
        // Class 1: land side in two adjacent ranges (merged), anywhere further out; class 2: water.
        let rules = [rule(1, 0x01, 50000, 20000), rule(1, 0x11, 19999, 3000), rule(1, 0x11, 2999, 1), rule(2, 0x21, 50000, 1), rule(3, 0x10, 50000, 1)];
        let r = interactive_ranges(&rules, false);
        let sides = |c: u16| r[&c].iter().map(|e| (e.2, e.3)).collect::<Vec<_>>();
        assert_eq!(sides(1), [(1, 0), (1, 1)]);
        assert_eq!(sides(2), [(1, 2)]);
        assert!(!r.contains_key(&3), "tri-state 0: not interactive");
        assert!(interactive_ranges(&rules, true).values().flatten().all(|e| e.3 == 0), "Moon polygons are not split");
    }

    fn class(id: u16, pen_group: u16, flag: u8, scale: u32, name: &str) -> FeatureClass {
        FeatureClass { id, kind: GeometryKind::Area, pen_group, flag, scale, reserved: 0, group: 0, priority: 100, name: name.into() }
    }

    #[test]
    fn menu_exclusion_is_by_row_not_class_id() {
        // Class ids differ from rows, as on the disc; the tiles' `class` is the row.
        let classes = [class(500, 6, 0, 0, "A"), class(501, MENU_EXCLUDED_PEN_GROUP, 0, 0, "Zone"), class(9, MENU_EXCLUDED_PEN_GROUP, 0, 0, "MOON zone")];
        let moon = |row: usize| classes[row].name.starts_with("MOON");
        assert_eq!(menu_excluded_rows(&classes, moon, false), [1]);
        assert_eq!(menu_excluded_rows(&classes, moon, true), [2]);
    }

    #[test]
    fn class_hit_order_by_row() {
        let classes = [class(500, 6, 0, 250_000, "Region"), class(501, 6, 1, 400_000, "Road"), class(502, 6, 0, 420_000, "Country")];
        let (scale, routable) = class_hit_order(&classes, [0u16, 1, 7].into_iter());
        assert_eq!(Value::Object(scale), json!({ "0": 250, "1": 400 }));
        assert_eq!(routable, [1]);
    }

    #[test]
    fn point_label_candidates_in_mobb_order() {
        // Above right, below right, above left, below left; centred first without a symbol.
        assert_eq!(point_label_anchors(true), json!(["bottom-left", "top-left", "bottom-right", "top-right"]));
        assert_eq!(point_label_anchors(false), json!(["center", "bottom-left", "top-left", "bottom-right", "top-right"]));
    }

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
    fn alternative_name_rule_meets_at_the_main_descender() {
        // 14 px main name, 12 px alternative, letter spacing 2 glyph units.
        let (main, alt) = rule_faces("Arial Bold", "Arial Regular", 12.0 / 14.0, 2.0);
        assert_eq!(main, "Arial Bold Rule 24.00 2.00 1.00");
        assert_eq!(alt, "Arial Regular Rule -5.60 2.33 1.17");
        // The alternative's line starts 28.8 main units lower: −5.6 × 12/14 + 28.8 = 24.
        let text = TextSpec { upper: false, alt: Some(AltLine { scale: 0.5, colour: "#000000".into(), upper: false, main_rule_face: main.clone(), alt_rule_face: alt.clone() }) };
        let f = text.with_alt(text.name()).to_string();
        assert!(f.contains(&main) && f.contains(&alt) && f.contains("\"font-scale\":0.5"), "{f}");
        assert_eq!(text.max_width(8.0), json!(["case", ["has", "alt_names"], 1000, 8.0]));
    }

    #[test]
    fn names() {
        assert_eq!(slug("Atlas Default-No HS"), "atlas-default-no-hs");
        let fonts = BTreeMap::from([(9u8, "MS Reference Sans Serif".to_string())]);
        assert_eq!(face_name(&fonts, 9, true, false), "MS Reference Sans Serif Bold");
    }
}
