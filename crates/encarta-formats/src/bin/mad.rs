//! Dumps and checks the MAD atlas database (docs/formats/mad.md).
//!
//! mad toc <file.MAD>                                       header and sections of one file
//! mad table <name> [--key K] [--limit N] [--json]          a plain table, or grid records
//! mad tile <raster> <level> <x> <y> --png OUT [--look STYLE|index] [--stored]
//! mad raster <raster> <level> --png OUT [--look STYLE|index] [--equirect W [--bbox W,S,E,N] [--overlay F.geojson]]
//! mad geom <cell|cell:K|entity:ID|bbox:W,S,E,N> [--band N] [--class TEXT] --geojson [OUT]
//! mad entity <id> [--json]                                 gazetteer record
//! mad check [--all]                                        decode everything, compare with the spec
//!
//! Common options: --db WLD|SWLD (default WLD), --extracted DIR (default $ENCARTA_EXTRACTED, then
//! `extracted`), --hd FILE --cd FILE (explicit halves), --lcid HEX (default 0409).
//! `--look index` writes raster values as grey levels; tiles are written in image orientation
//! unless `--stored` is given.

use anyhow::{Context, Result, bail};
use encarta_formats::mad::cells::{Cell, CellSystem};
use encarta_formats::mad::rmap::{self, TILE};
use encarta_formats::mad::schemas::{self, GeoGemRecord, GeoObjRecord, ObjectRef};
use encarta_formats::mad::tables::{self, FeatureClass, GeometryKind};
use encarta_formats::mad::{Atlas, Edition, gazetteer::Gazetteer};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `println!` that returns an error instead of panicking when stdout is closed.
macro_rules! outln {
    ($($t:tt)*) => {{
        let mut w = std::io::stdout().lock();
        writeln!(w, $($t)*)?;
    }};
}

const USAGE: &str = "usage: mad <toc|table|tile|raster|geom|entity|check> … (see the source header for options)";

struct Opts {
    pos: Vec<String>,
    flags: HashMap<String, Option<String>>,
}

/// Flags that take no value.
const SWITCHES: &[&str] = &["--json", "--all", "--stored", "--force", "--geojson", "--pretty"];

impl Opts {
    fn parse(args: &[String]) -> Opts {
        let mut pos = Vec::new();
        let mut flags = HashMap::new();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if a.starts_with("--") {
                if SWITCHES.contains(&a.as_str()) {
                    // `--geojson OUT` may carry an output path.
                    if a == "--geojson" && args.get(i + 1).is_some_and(|n| !n.starts_with("--")) {
                        flags.insert(a.clone(), Some(args[i + 1].clone()));
                        i += 1;
                    } else {
                        flags.insert(a.clone(), None);
                    }
                } else {
                    flags.insert(a.clone(), args.get(i + 1).cloned());
                    i += 1;
                }
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        Opts { pos, flags }
    }
    fn has(&self, f: &str) -> bool {
        self.flags.contains_key(f)
    }
    fn get(&self, f: &str) -> Option<&str> {
        self.flags.get(f).and_then(|v| v.as_deref())
    }
    fn arg(&self, i: usize, what: &str) -> Result<&str> {
        self.pos.get(i).map(String::as_str).with_context(|| format!("missing {what}\n{USAGE}"))
    }
    fn num(&self, i: usize, what: &str) -> Result<u32> {
        parse_num(self.arg(i, what)?)
    }
    fn lcid(&self) -> Result<u16> {
        Ok(u16::from_str_radix(self.get("--lcid").unwrap_or("0409"), 16)?)
    }
}

fn parse_num(s: &str) -> Result<u32> {
    let v = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16)?,
        None => s.parse()?,
    };
    Ok(v)
}

fn extracted(o: &Opts) -> PathBuf {
    o.get("--extracted")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ENCARTA_EXTRACTED").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("extracted"))
}

fn edition(o: &Opts) -> Result<Edition> {
    let s = o.get("--db").unwrap_or("WLD");
    Edition::parse(s).with_context(|| format!("unknown database {s:?} (WLD or SWLD)"))
}

fn open(o: &Opts) -> Result<Atlas> {
    if let (Some(hd), Some(cd)) = (o.get("--hd"), o.get("--cd")) {
        return Atlas::open(&[PathBuf::from(hd), PathBuf::from(cd)]);
    }
    Atlas::open_edition(&extracted(o), edition(o)?)
}

fn emit<T: Serialize>(v: &T, pretty: bool) -> Result<()> {
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    let r = if pretty { serde_json::to_writer_pretty(&mut w, v) } else { serde_json::to_writer(&mut w, v) };
    let r = r.map_err(std::io::Error::from).and_then(|()| writeln!(w)).and_then(|()| w.flush());
    match r {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

fn write_png(path: &str, width: usize, height: usize, rgba: &[u8]) -> Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let f = std::fs::File::create(path).with_context(|| format!("creating {path}"))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), width as u32, height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    eprintln!("wrote {path} ({width}×{height})");
    Ok(())
}

fn colourise(r: &rmap::Raster, look: Option<&str>, pixels: &[u8]) -> Result<Vec<u8>> {
    match look {
        Some(l) if l.eq_ignore_ascii_case("index") => Ok(pixels.iter().flat_map(|&v| [v, v, v, 255]).collect()),
        Some(l) => Ok(r.look(l).with_context(|| format!("{} has no LOOK {l:?}", r.name))?.to_rgba(pixels)),
        None => Ok(r.looks.first().context("raster has no LOOK styles")?.to_rgba(pixels)),
    }
}

// ---------------------------------------------------------------------------------------------

fn cmd_toc(o: &Opts) -> Result<()> {
    let f = encarta_formats::mad::MadFile::open(Path::new(o.arg(0, "file")?))?;
    if o.has("--json") {
        return emit(&f.toc, true);
    }
    outln!("version {:?}, stamp {:#010x}, {} sections", f.toc.header.version, f.toc.header.stamp, f.toc.sections.len());
    for s in &f.toc.sections {
        let b = f.section(&s.name).unwrap();
        let kind = if b.starts_with(b"INDX") {
            "INDX"
        } else if b.starts_with(b"RMAP") {
            "RMAP"
        } else {
            ""
        };
        outln!("{:>11} {:>11}  {:<28} {kind}", s.offset, s.length, s.name);
    }
    Ok(())
}

/// Decodes the grid records named by `--key` (or all, up to `--limit`) as JSON values.
fn grid_records<T: schemas::GridSchema + Serialize + Send>(grid: &encarta_formats::mad::Grid, o: &Opts) -> Result<Value> {
    let keys: Vec<u32> = match o.get("--key") {
        Some(k) => vec![parse_num(k)?],
        None => {
            let limit = o.get("--limit").map(parse_num).transpose()?.unwrap_or(u32::MAX) as usize;
            grid.index.records().iter().map(|r| r.2).take(limit).collect()
        }
    };
    let v: Vec<Value> = keys
        .par_iter()
        .map(|&k| Ok(json!({ "key": k, "record": serde_json::to_value(schemas::decode_record::<T>(grid, k)?)? })))
        .collect::<Result<_>>()?;
    Ok(Value::Array(v))
}

fn cmd_table(o: &Opts) -> Result<()> {
    let a = open(o)?;
    let name = o.arg(0, "table name")?;
    let b = a.require(name)?;
    let base = lcid_base(name);
    let g1 = || a.g1g2();
    let v: Value = if b.starts_with(b"INDX") {
        let idx = a.index(name)?;
        json!({ "pages": idx.last_keys.len(), "keys": idx.key_count(), "records": idx.records().len(),
                 "runs": idx.runs().iter().take(o.get("--limit").map(parse_num).transpose()?.unwrap_or(100) as usize)
                    .map(|r| json!([r.first, r.count, r.offset, r.length])).collect::<Vec<_>>() })
    } else if name == "Terrain" {
        let t = a.terrain()?;
        json!({ "rasters": t.rasters.iter().map(|r| r.info()).collect::<Vec<_>>(), "mesh_vertices": t.mesh_vertices.len() })
    } else if name.starts_with("Find Grid") {
        let grid = a.grid_named(name)?;
        let g = g1()?;
        let mut v = grid_records::<schemas::FindPage>(&grid, o)?;
        // Add the decoded keys, which the record type keeps as bytes.
        if let Value::Array(items) = &mut v {
            for item in items {
                let key = item["key"].as_u64().unwrap() as u32;
                let page: schemas::FindPage = schemas::decode_record(&grid, key)?;
                for (e, j) in page.entries.iter().zip(item["record"]["entries"].as_array_mut().unwrap()) {
                    j["key"] = json!(g.decode(&e.key));
                }
            }
        }
        v
    } else if let Ok(grid) = a.grid_named(name) {
        match base {
            "LegendGrid" => grid_records::<schemas::Legend>(&grid, o)?,
            "Bitmap" => grid_records::<schemas::Bitmap>(&grid, o)?,
            "EntityConv" => grid_records::<schemas::EntityConvPage>(&grid, o)?,
            "PDCAltitudeGrid" => grid_records::<schemas::DisplayRules>(&grid, o)?,
            "MillerTerrainGrid" => grid_records::<schemas::MillerTile>(&grid, o)?,
            "FindGECContextGrid" => grid_records::<schemas::FindContextPage>(&grid, o)?,
            "GeoObjGrid" => grid_records::<GeoObjRecord>(&grid, o)?,
            "GeoGemGrid" => {
                let kinds = a.class_kinds()?;
                let key = parse_num(o.get("--key").context("GeoGemGrid needs --key (or use `mad geom`)")?)?;
                let mut r = grid.record(key)?;
                let g = GeoGemRecord::decode(&mut r, &kinds)?;
                json!({ "key": key, "record": g, "x": g.x, "y": g.y, "unused_bits": r.remaining_bits() })
            }
            _ => bail!("no schema for {name}"),
        }
    } else {
        match name {
            "PDCAttribute" => serde_json::to_value(tables::pdc_attribute(b)?)?,
            "GeoFaceScale" => serde_json::to_value(a.cells()?)?,
            "LangId" => serde_json::to_value(tables::lang_ids(b)?)?,
            "EntityCenter" => serde_json::to_value(tables::entity_centers(b)?)?,
            "EntityConvKey" => serde_json::to_value(tables::key_table(b)?)?,
            "BitmapPalette" => serde_json::to_value(tables::bitmap_palette(b)?)?,
            "BitmapColor" => serde_json::to_value(tables::bitmap_color(b)?)?,
            "BitmapDimDir" => serde_json::to_value(tables::bitmap_dims(b)?)?,
            "MapStyle" => serde_json::to_value(tables::map_styles(b)?)?,
            "GeoPenDef" => serde_json::to_value(tables::pen_defs(b)?)?,
            "GeoLabel" => serde_json::to_value(tables::label_styles(b)?)?,
            "Label Font" => serde_json::to_value(tables::label_fonts(b)?)?,
            "ShieldId" => serde_json::to_value(tables::shield_ids(b)?)?,
            "MillerStyles" => serde_json::to_value(tables::miller_styles(b)?)?,
            "GeoTrans" => serde_json::to_value(tables::geo_trans(b)?)?,
            "MinRect" => serde_json::to_value(tables::min_rects(b)?)?,
            "Unicode Attributes" => serde_json::to_value(tables::unicode_attributes(b)?)?,
            "Font Conv" => serde_json::to_value(tables::font_conv(b)?)?,
            "G1G2 to Unicode" => {
                let g = g1()?;
                let all: Vec<u8> = (0x20..=0xFF).collect();
                let line = |prefix: &[u8]| {
                    let mut s = prefix.to_vec();
                    s.extend(all.iter().filter(|&&c| c != 0));
                    g.decode(&s)
                };
                json!({ "map0": line(b""), "map1": line(b"\x03"), "map2": line(b"\x04") })
            }
            _ if base == "Find Key" => serde_json::to_value(tables::key_table(b)?)?,
            _ if base == "Find Data" => serde_json::to_value(tables::find_data(b, &g1()?)?)?,
            _ => bail!("no reader for table {name:?} (raw section of {} bytes)", b.len()),
        }
    };
    if o.has("--json") {
        emit(&v, o.has("--pretty"))
    } else {
        emit(&v, true)
    }
}

/// The table name without its 4-hex-digit language suffix, if it has one.
fn lcid_base(name: &str) -> &str {
    let n = name.len();
    if n > 4 && name.is_char_boundary(n - 4) && name[n - 4..].bytes().all(|c| c.is_ascii_hexdigit()) {
        let base = &name[..n - 4];
        if ["Find Key", "Find Data", "Find Grid", "Find Ndx", "GeoObjGrid", "GeoObjLookup", "FindGECContextGrid", "FindGECContextLookup"]
            .contains(&base)
        {
            return base;
        }
    }
    name
}

fn cmd_tile(o: &Opts) -> Result<()> {
    let a = open(o)?;
    let t = a.terrain()?;
    let name = o.arg(0, "raster")?;
    let r = t.raster(name).with_context(|| format!("no raster {name}"))?;
    let (level, x, y) = (o.num(1, "level")?, o.num(2, "x")?, o.num(3, "y")?);
    let key = rmap::tile_key(level, x, y);
    let d = r.decode_key(key)?;
    eprintln!("{} tile {key:#010x} {:?}: mode {:?}, {} padding bits", r.name, r.index.lookup(key), d.mode, d.padding_bits);
    let px = if o.has("--stored") { d.pixels } else { rmap::tile_in_image_orientation(level, x, y, &d.pixels) };
    let out = o.get("--png").context("--png OUT required")?;
    write_png(out, TILE, TILE, &colourise(r, o.get("--look"), &px)?)
}

/// Draws GeoJSON lines (LineString / Polygon / Multi*) onto an RGBA image showing the
/// latitude/longitude window `win` = (west, south, east, north).
fn draw_overlay(rgba: &mut [u8], w: usize, h: usize, win: (f64, f64, f64, f64), geojson: &Value) {
    let to_px = |p: &Value| -> Option<(f64, f64)> {
        let lon = p.get(0)?.as_f64()?;
        let lat = p.get(1)?.as_f64()?;
        Some(((lon - win.0) / (win.2 - win.0) * w as f64, (win.3 - lat) / (win.3 - win.1) * h as f64))
    };
    let plot = |rgba: &mut [u8], (x, y): (f64, f64)| {
        let (x, y) = (x as i64, y as i64);
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = (y as usize * w + x as usize) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    };
    let line = |rgba: &mut [u8], pts: &[Value]| {
        for s in pts.windows(2) {
            let (Some(a), Some(b)) = (to_px(&s[0]), to_px(&s[1])) else { continue };
            if (a.0 - b.0).abs() > w as f64 / 2.0 {
                continue; // crosses the antimeridian
            }
            let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1);
            for k in 0..=n {
                let t = k as f64 / n as f64;
                plot(rgba, (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
    };
    for f in geojson["features"].as_array().into_iter().flatten() {
        let g = &f["geometry"];
        let c = &g["coordinates"];
        match g["type"].as_str() {
            Some("LineString") => line(rgba, c.as_array().unwrap()),
            Some("Polygon") | Some("MultiLineString") => c.as_array().unwrap().iter().for_each(|r| line(rgba, r.as_array().unwrap())),
            Some("Point") => {
                if let Some(p) = to_px(c) {
                    plot(rgba, p);
                }
            }
            _ => {}
        }
    }
}

fn cmd_raster(o: &Opts) -> Result<()> {
    let a = open(o)?;
    let t = a.terrain()?;
    let name = o.arg(0, "raster")?;
    let r = t.raster(name).with_context(|| format!("no raster {name}"))?;
    let level = o.num(1, "level")?;
    let side = rmap::image_side(level);
    if side > 16_256 && !o.has("--force") {
        bail!("level {level} is {side}×{side} pixels; pass --force to assemble it anyway");
    }
    let t0 = Instant::now();
    let image = r.assemble(level)?;
    eprintln!("assembled {} level {level} ({side}×{side}) in {:.2} s", r.name, t0.elapsed().as_secs_f64());
    let out = o.get("--png").context("--png OUT required")?;
    if let Some(w) = o.get("--equirect") {
        let w = parse_num(w)? as usize;
        let win = match o.get("--bbox") {
            Some(b) => {
                let v: Vec<f64> = b.split(',').map(|s| s.trim().parse()).collect::<std::result::Result<_, _>>()?;
                let [w, s, e, n] = v[..] else { bail!("--bbox needs W,S,E,N") };
                (w, s, e, n)
            }
            None => (-180.0, -90.0, 180.0, 90.0),
        };
        // Square degrees: the height follows the window's aspect ratio.
        let h = ((w as f64) * (win.3 - win.1) / (win.2 - win.0)).round().max(1.0) as usize;
        let eq = rmap::octahedral_to_window(&image, side, w, h, win);
        let mut rgba = colourise(r, o.get("--look"), &eq)?;
        if let Some(path) = o.get("--overlay") {
            let gj: Value = serde_json::from_slice(&std::fs::read(path)?)?;
            draw_overlay(&mut rgba, w, h, win, &gj);
        }
        write_png(out, w, h, &rgba)
    } else {
        write_png(out, side, side, &colourise(r, o.get("--look"), &image)?)
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry

struct GeomContext<'a> {
    grid: encarta_formats::mad::Grid<'a>,
    cells: CellSystem,
    classes: Vec<FeatureClass>,
    kinds: Vec<GeometryKind>,
    geo_obj: encarta_formats::mad::Grid<'a>,
    names: std::sync::Mutex<HashMap<u32, Vec<(u32, String)>>>,
}

impl GeomContext<'_> {
    fn object(&self, r: ObjectRef) -> (u32, String) {
        let mut cache = self.names.lock().unwrap();
        let rec = cache.entry(r.key).or_insert_with(|| {
            schemas::decode_record::<GeoObjRecord>(&self.geo_obj, r.key)
                .map(|g| g.entries.into_iter().map(|e| (e.entity, e.name)).collect())
                .unwrap_or_default()
        });
        rec.get(r.entry as usize).cloned().unwrap_or_default()
    }

    /// GeoJSON features of the objects of record `g` placed in cell `key` that pass `keep`.
    fn features(&self, key: u32, g: &GeoGemRecord, keep: &dyn Fn(ObjectRef, &FeatureClass) -> bool) -> Vec<Value> {
        let Some(cell): Option<Cell> = self.cells.cell(key) else { return Vec::new() };
        let mut out = Vec::new();
        for obj in &g.objects {
            for part in &obj.parts {
                let class = &self.classes[part.class as usize];
                if !keep(obj.object, class) {
                    continue;
                }
                let pts: Vec<Value> = g
                    .points(part)
                    .map(|(x, y)| {
                        let (lat, lon) = cell.lat_lon(x as f64, y as f64);
                        json!([(lon * 1e6).round() / 1e6, (lat * 1e6).round() / 1e6])
                    })
                    .collect();
                let geometry = match self.kinds[part.class as usize] {
                    GeometryKind::Point | GeometryKind::RadiusPoint => json!({ "type": "Point", "coordinates": pts[0] }),
                    GeometryKind::Area if pts.len() >= 3 => {
                        let mut ring = pts.clone();
                        if ring.first() != ring.last() {
                            ring.push(ring[0].clone());
                        }
                        json!({ "type": "Polygon", "coordinates": [ring] })
                    }
                    _ if pts.len() >= 2 => json!({ "type": "LineString", "coordinates": pts }),
                    _ => json!({ "type": "Point", "coordinates": pts[0] }),
                };
                let (entity, name) = self.object(obj.object);
                out.push(json!({
                    "type": "Feature",
                    "geometry": geometry,
                    "properties": { "cell": key, "band": cell.band, "object": [obj.object.key, obj.object.entry],
                                    "entity": entity, "name": name, "class": part.class, "class_name": class.name },
                }));
            }
        }
        out
    }
}

fn cmd_geom(o: &Opts) -> Result<()> {
    let a = open(o)?;
    let lcid = o.lcid()?;
    let classes = a.classes()?;
    let ctx = GeomContext {
        grid: a.geo_gem()?,
        cells: a.cells()?,
        kinds: classes.iter().map(|c| c.kind).collect(),
        classes,
        geo_obj: a.geo_obj(lcid)?,
        names: Default::default(),
    };
    let what = o.arg(0, "cell key, entity:ID or bbox:W,S,E,N")?;
    let class_filter = o.get("--class").map(str::to_ascii_lowercase);
    let class_ok = |c: &FeatureClass| class_filter.as_ref().is_none_or(|f| c.name.to_ascii_lowercase().contains(f));
    // Key runs grouped by record, so shared records are decoded once and placed in every cell.
    let decode = |off: u32, len: u32| -> Result<GeoGemRecord> {
        let mut r = ctx.grid.decoder(ctx.grid.bytes(off, len)?);
        GeoGemRecord::decode(&mut r, &ctx.kinds)
    };
    let features: Vec<Value> = if let Some(id) = what.strip_prefix("entity:") {
        let id = parse_num(id)?;
        let gz = Gazetteer::open(&a, lcid)?;
        let loc = gz.locate(id)?.with_context(|| format!("entity {id} not found"))?;
        let target = ObjectRef { key: loc.geoobj_key, entry: loc.entry as u32 };
        let mut by_record: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for run in ctx.grid.index.runs() {
            by_record.entry((run.offset, run.length)).or_default().extend(run.first..=run.last());
        }
        let records: Vec<_> = by_record.into_iter().collect();
        records
            .par_iter()
            .map(|((off, len), keys)| -> Result<Vec<Value>> {
                let g = decode(*off, *len)?;
                if !g.objects.iter().any(|ob| ob.object == target) {
                    return Ok(Vec::new());
                }
                Ok(keys.iter().flat_map(|&k| ctx.features(k, &g, &|r, c| r == target && class_ok(c))).collect())
            })
            .collect::<Result<Vec<_>>>()?
            .concat()
    } else if let Some(b) = what.strip_prefix("bbox:") {
        let v: Vec<f64> = b.split(',').map(|s| s.trim().parse()).collect::<std::result::Result<_, _>>()?;
        let [w, s, e, n] = v[..] else { bail!("bbox needs W,S,E,N") };
        let band = o.get("--band").map(parse_num).transpose()?.unwrap_or(5) as usize;
        let bandinfo = *ctx.cells.bands.get(band).context("no such band")?;
        let end = bandinfo.first_key + bandinfo.key_count();
        let keys: Vec<(u32, u32, u32)> = ctx
            .grid
            .index
            .runs()
            .iter()
            .flat_map(|r| (r.first..=r.last()).map(move |k| (k, r.offset, r.length)))
            .filter(|&(k, _, _)| k >= bandinfo.first_key && k < end)
            .filter(|&(k, _, _)| {
                let (cw, cs, ce, cn) = ctx.cells.cell(k).unwrap().bounds();
                cw <= e && ce >= w && cs <= n && cn >= s
            })
            .collect();
        eprintln!("band {band}: {} cells intersect the box", keys.len());
        keys.par_iter()
            .map(|&(k, off, len)| Ok(ctx.features(k, &decode(off, len)?, &|_, c| class_ok(c))))
            .collect::<Result<Vec<_>>>()?
            .concat()
    } else {
        let key = parse_num(what.strip_prefix("cell:").unwrap_or(what))?;
        let (off, len) = ctx.grid.index.lookup(key).with_context(|| format!("no GeoGem record for cell {key}"))?;
        ctx.features(key, &decode(off, len)?, &|_, c| class_ok(c))
    };
    eprintln!("{} features", features.len());
    let fc = json!({ "type": "FeatureCollection", "features": features });
    match o.get("--geojson") {
        Some(path) => {
            std::fs::write(path, serde_json::to_vec(&fc)?)?;
            eprintln!("wrote {path}");
            Ok(())
        }
        None => emit(&fc, false),
    }
}

fn cmd_entity(o: &Opts) -> Result<()> {
    let a = open(o)?;
    let gz = Gazetteer::open(&a, o.lcid()?)?;
    let id = o.num(0, "entity id")?;
    let e = gz.entity(id)?;
    if o.has("--json") {
        return emit(&e, true);
    }
    outln!("entity {}: {}", e.id, e.names.join(" / "));
    if let Some((t, d)) = &e.find_type {
        outln!("  type     {t} {d:?}");
    }
    if let Some((c, d)) = &e.context {
        outln!("  context  {c} {d:?}");
    }
    outln!("  GeoObj   key {} entry {} (class {}, scale value {}, label {:?})", e.geoobj_key, e.geoobj_entry, e.class, e.scale, e.label);
    if let (Some(c), Some(b)) = (e.cell, e.cell_bounds) {
        outln!("  cell     band {} octant {} row {} col {}; {:.3}…{:.3} E, {:.3}…{:.3} N", c.band, c.octant, c.row, c.col, b.0, b.2, b.1, b.3);
    }
    match e.centroid {
        Some((lat, lon)) => outln!("  centroid {:.4}° {}, {:.4}° {}", lat.abs(), if lat < 0.0 { 'S' } else { 'N' }, lon.abs(), if lon < 0.0 { 'W' } else { 'E' }),
        None => outln!("  centroid (not in EntityCenter)"),
    }
    for k in &e.search_keys {
        outln!("  key      {k:?}");
    }
    Ok(())
}

fn cmd_check(o: &Opts) -> Result<()> {
    let editions: Vec<Edition> =
        if o.has("--all") && !o.has("--db") { vec![Edition::World, Edition::Student] } else { vec![edition(o)?] };
    let t0 = Instant::now();
    let mut failures = 0;
    for ed in editions {
        let t = Instant::now();
        let a = if o.has("--hd") { open(o)? } else { Atlas::open_edition(&extracted(o), ed)? };
        outln!("== {} ({})", ed.prefix(), a.files.iter().map(|f| f.path.display().to_string()).collect::<Vec<_>>().join(" + "));
        let rep = encarta_formats::mad::check::run(&a, Some(ed));
        for l in &rep.log {
            outln!("{l}");
        }
        outln!("== {}: {} failures, {:.1} s", ed.prefix(), rep.failures.len(), t.elapsed().as_secs_f64());
        failures += rep.failures.len();
    }
    outln!("total {:.1} s", t0.elapsed().as_secs_f64());
    if failures > 0 {
        bail!("{failures} checks failed");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { bail!(USAGE) };
    let o = Opts::parse(&args[1..]);
    let r = match cmd.as_str() {
        "toc" => cmd_toc(&o),
        "table" => cmd_table(&o),
        "tile" => cmd_tile(&o),
        "raster" => cmd_raster(&o),
        "geom" => cmd_geom(&o),
        "entity" => cmd_entity(&o),
        "check" => cmd_check(&o),
        _ => bail!(USAGE),
    };
    // A closed pipe (e.g. `| head`) is not an error for a dump tool.
    match r {
        Err(e) if e.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe) => Ok(()),
        other => other,
    }
}
