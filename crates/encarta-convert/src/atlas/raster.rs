//! RMAP rasters → Web Mercator PMTiles (one WebP archive per LOOK that a MapStyle uses, plus an
//! 8-bit class-index PNG archive per raster) and equirectangular globe textures.
//!
//! Sampling is nearest-neighbour on raster values (class indices), then the LOOK's CMT colours.
//! Each output row has one latitude, so each row picks the octahedral pyramid level whose pixel is
//! not coarser than the output pixel (Encarta's own coarser levels are its generalisation). Source
//! tiles are decoded on demand into a small per-thread cache, so no level mosaic is held in memory.

use super::geo;
use super::pmtiles::{self, Info, TileType, Writer};
use anyhow::{Context, Result};
use encarta_formats::mad::octahedral::from_lat_lon;
use encarta_formats::mad::rmap::{self, Look, Raster};
use encarta_formats::mad::tables::MapStyle;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const TILE_SIZE: usize = 256;

/// The coarsest level whose pixels are not (much) coarser than `res` radians.
pub fn level_for(res: f64, max_level: u32) -> u32 {
    let l = (std::f64::consts::FRAC_PI_2 / (rmap::TILE as f64 * res)).log2() - 0.25;
    (l.ceil().max(0.0) as u32).min(max_level)
}

/// Highest Mercator zoom that still gains detail from the finest level (256-px tiles, equator).
pub fn max_zoom(max_level: u32) -> u8 {
    let mut z = 0u8;
    while level_for(2.0 * std::f64::consts::PI / (TILE_SIZE as f64 * (1u64 << (z + 1)) as f64), u32::MAX) <= max_level {
        z += 1;
    }
    z
}

/// Decoded source tiles of one raster (per thread).
pub struct TileCache<'r, 'a> {
    raster: &'r Raster<'a>,
    map: HashMap<u32, (Arc<Vec<u8>>, u64)>,
    clock: u64,
    cap: usize,
    last: Option<(u32, Arc<Vec<u8>>)>,
}

impl<'r, 'a> TileCache<'r, 'a> {
    pub fn new(raster: &'r Raster<'a>, cap: usize) -> Self {
        TileCache { raster, map: HashMap::new(), clock: 0, cap, last: None }
    }

    fn tile(&mut self, key: u32) -> Result<Arc<Vec<u8>>> {
        if let Some((k, t)) = &self.last
            && *k == key
        {
            return Ok(t.clone());
        }
        self.clock += 1;
        let clock = self.clock;
        let t = if let Some(e) = self.map.get_mut(&key) {
            e.1 = clock;
            e.0.clone()
        } else {
            let t = Arc::new(self.raster.decode_key(key)?.pixels);
            if self.map.len() >= self.cap {
                // Drop the older half.
                let mut stamps: Vec<u64> = self.map.values().map(|v| v.1).collect();
                stamps.sort_unstable();
                let cut = stamps[stamps.len() / 2];
                self.map.retain(|_, v| v.1 > cut);
            }
            self.map.insert(key, (t.clone(), clock));
            t
        };
        self.last = Some((key, t.clone()));
        Ok(t)
    }

    /// `class[value]` of every pixel of source tile `(x, y)` at `level`, if it is the same for all.
    pub fn uniform_tile(&mut self, level: u32, x: u32, y: u32, class: &[bool; 256]) -> Result<Option<bool>> {
        let t = self.tile(rmap::tile_key(level, x, y))?;
        let v = class[t[0] as usize];
        Ok(t.iter().all(|&p| class[p as usize] == v).then_some(v))
    }

    /// Raster value of octahedral image pixel `(row, col)` at `level`.
    pub fn pixel(&mut self, level: u32, row: usize, col: usize) -> Result<u8> {
        let (x, y, i) = rmap::image_to_tile_pixel(level, row, col);
        Ok(self.tile(rmap::tile_key(level, x, y))?[i])
    }

    /// Fills `out` with the values along latitude `lat` at longitudes `lon0 + (i + 0.5)·dlon`.
    /// The projection is evaluated exactly every 8 pixels and at octant boundaries, and
    /// interpolated linearly in between (the mapping is smooth inside one octant face).
    pub fn sample_row(&mut self, level: u32, lat: f64, lon0: f64, dlon: f64, out: &mut [u8]) -> Result<()> {
        let side = rmap::image_side(level);
        let h = side as f64 / 2.0;
        let w = out.len();
        let lon_at = |i: usize| lon0 + (i as f64 + 0.5) * dlon;
        let quadrant = |lon: f64| ((lon + 180.0).rem_euclid(360.0) / 90.0).floor() as i32;
        let uv = |lon: f64| from_lat_lon(lat, lon, 1.0);
        let to_px = |u: f64, v: f64| {
            let c = (u * h + h).floor().clamp(0.0, side as f64 - 1.0) as usize;
            let r = (v * h + h).floor().clamp(0.0, side as f64 - 1.0) as usize;
            (r, c)
        };
        let mut i = 0usize;
        while i < w {
            let j = (i + 8).min(w - 1);
            let (la, lb) = (lon_at(i), lon_at(j));
            if j > i && quadrant(la) == quadrant(lb) {
                let (ua, va) = uv(la);
                let (ub, vb) = uv(lb);
                let span = (j - i) as f64;
                for (k, o) in out.iter_mut().enumerate().take(j).skip(i) {
                    let t = (k - i) as f64 / span;
                    let (r, c) = to_px(ua + (ub - ua) * t, va + (vb - va) * t);
                    *o = self.pixel(level, r, c)?;
                }
            } else {
                for (k, o) in out.iter_mut().enumerate().take(j.max(i + 1)).skip(i) {
                    let (u, v) = uv(lon_at(k));
                    let (r, c) = to_px(u, v);
                    *o = self.pixel(level, r, c)?;
                }
            }
            if j == w - 1 {
                let (u, v) = uv(lon_at(j));
                let (r, c) = to_px(u, v);
                out[j] = self.pixel(level, r, c)?;
                break;
            }
            i = j;
        }
        Ok(())
    }

    /// The class-index image of Mercator tile `(z, x, y)` (256 × 256, row-major).
    pub fn mercator_tile(&mut self, z: u8, x: u32, y: u32, max_level: u32) -> Result<Vec<u8>> {
        let mut out = vec![0u8; TILE_SIZE * TILE_SIZE];
        let n = (1u64 << z) as f64 * TILE_SIZE as f64;
        let dlon = 360.0 / n;
        let lon0 = x as f64 * TILE_SIZE as f64 * dlon - 180.0;
        for (py, row) in out.chunks_mut(TILE_SIZE).enumerate() {
            let lat = geo::y_to_lat((y as f64 * TILE_SIZE as f64 + py as f64 + 0.5) / n);
            let res = 2.0 * std::f64::consts::PI * lat.to_radians().cos() / n;
            self.sample_row(level_for(res, max_level), lat, lon0, dlon, row)?;
        }
        Ok(out)
    }
}

/// How a LOOK's colour tiles are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Lossless,
    /// Lossy, unless lossless comes out smaller for that tile.
    Lossy,
}

/// Photographic rasters get lossy WebP; class maps stay lossless.
fn encoding_for(raster: &str) -> Encoding {
    match raster.to_ascii_uppercase().as_str() {
        "SATELLITE" | "NIGHT" | "MOON" => Encoding::Lossy,
        _ => Encoding::Lossless,
    }
}

pub fn webp_lossless(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut cfg = webp::WebPConfig::new().expect("webp config");
    cfg.lossless = 1;
    cfg.quality = 25.0;
    cfg.method = 2;
    webp::Encoder::from_rgb(rgb, w, h).encode_advanced(&cfg).expect("webp lossless encode").to_vec()
}

pub fn webp_lossy(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut cfg = webp::WebPConfig::new().expect("webp config");
    cfg.quality = 85.0;
    cfg.method = 4;
    webp::Encoder::from_rgb(rgb, w, h).encode_advanced(&cfg).expect("webp lossy encode").to_vec()
}

fn encode_look(look: &Look, idx: &[u8], w: u32, h: u32, enc: Encoding) -> Vec<u8> {
    let rgb: Vec<u8> = idx.iter().flat_map(|&v| look.values[v as usize].rgb).collect();
    let ll = webp_lossless(&rgb, w, h);
    if enc == Encoding::Lossy {
        let lossy = webp_lossy(&rgb, w, h);
        if lossy.len() < ll.len() {
            return lossy;
        }
    }
    ll
}

pub fn png_gray(idx: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut e = png::Encoder::new(&mut buf, w, h);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Eight);
    e.set_compression(png::Compression::Balanced);
    e.write_header().and_then(|mut w| w.write_image_data(idx)).expect("png encode");
    buf
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveReport {
    pub path: String,
    pub raster: String,
    pub look: Option<String>,
    pub format: &'static str,
    pub encoding: Option<Encoding>,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub tiles: u64,
    pub distinct_tiles: u64,
    pub bytes: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct RasterReport {
    pub archives: Vec<ArchiveReport>,
    pub globe_textures: Vec<String>,
    /// LOOK styles present in the data that no MapStyle uses (palette exported only).
    pub unused_looks: Vec<(String, String)>,
    pub seconds: f64,
}

/// Which LOOKs each raster needs: those named by a MapStyle.
pub fn used_looks(styles: &[MapStyle]) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in styles {
        let v = m.entry(s.raster.to_ascii_uppercase()).or_default();
        if !v.iter().any(|l| l.eq_ignore_ascii_case(&s.look)) {
            v.push(s.look.clone());
        }
    }
    m
}

pub fn archive_name(raster: &str, look: Option<&str>) -> String {
    format!("raster/{}/{}.pmtiles", raster.to_ascii_lowercase(), look.map_or("index".to_string(), |l| l.to_ascii_lowercase()))
}

fn metadata(raster: &Raster, look: Option<&str>, max_zoom: u8, format: &str) -> Value {
    json!({
        "name": format!("Encarta {} {}", raster.name, look.unwrap_or("class index")),
        "format": format,
        "type": "baselayer",
        "attribution": "Microsoft Encarta Premium 2009 (converted from the user's disc)",
        "minzoom": 0, "maxzoom": max_zoom, "tileSize": TILE_SIZE,
        "encarta": {
            "raster": raster.name, "look": look, "source_max_level": raster.max_level,
            "min_altitude_km": raster.display_scale,
            "values": if look.is_none() { "8-bit raster values (CMT index); colour with palettes.json" } else { "RGB" },
        }
    })
}

/// Writes the Mercator archives of one raster.
fn raster_archives(raster: &Raster, looks: &[&Look], out: &Path, report: &mut RasterReport, log: &dyn Fn(String)) -> Result<()> {
    let maxz = max_zoom(raster.max_level);
    let enc = encoding_for(&raster.name);
    let idx_writer = Mutex::new(Writer::create(&out.join(archive_name(&raster.name, None)))?);
    let look_writers: Vec<Mutex<Writer>> =
        looks.iter().map(|l| Ok(Mutex::new(Writer::create(&out.join(archive_name(&raster.name, Some(&l.name))))?))).collect::<Result<_>>()?;
    for z in 0..=maxz {
        let n = 1u32 << z;
        let block = 8u32.min(n);
        let blocks: Vec<(u32, u32)> = (0..n / block).flat_map(|bx| (0..n / block).map(move |by| (bx, by))).collect();
        blocks.par_iter().try_for_each_init(
            || TileCache::new(raster, 1024),
            |cache, &(bx, by)| -> Result<()> {
                for x in bx * block..(bx + 1) * block {
                    for y in by * block..(by + 1) * block {
                        let idx = cache.mercator_tile(z, x, y, raster.max_level)?;
                        let png = png_gray(&idx, TILE_SIZE as u32, TILE_SIZE as u32);
                        idx_writer.lock().unwrap().add(z, x, y, &png)?;
                        for (look, w) in looks.iter().zip(&look_writers) {
                            let t = encode_look(look, &idx, TILE_SIZE as u32, TILE_SIZE as u32, enc);
                            w.lock().unwrap().add(z, x, y, &t)?;
                        }
                    }
                }
                Ok(())
            },
        )?;
    }
    let lat = geo::MAX_LAT;
    let mut finish = |w: Mutex<Writer>, look: Option<&str>, tile_type: TileType, encoding: Option<Encoding>| -> Result<()> {
        let path = archive_name(&raster.name, look);
        let format = if tile_type == TileType::Png { "png" } else { "webp" };
        let info = Info { tile_type, gzip_tiles: false, min_zoom: 0, max_zoom: maxz, bounds: (-180.0, -lat, 180.0, lat), center: (0.0, 20.0, 2) };
        let s = w.into_inner().unwrap().finish(&info, &metadata(raster, look, maxz, format))?;
        log(format!("  {path}: z0–{maxz}, {} tiles ({} distinct), {:.1} MB", s.addressed, s.contents, s.bytes as f64 / 1e6));
        report.archives.push(ArchiveReport {
            path,
            raster: raster.name.clone(),
            look: look.map(str::to_string),
            format,
            encoding,
            min_zoom: 0,
            max_zoom: maxz,
            tiles: s.addressed,
            distinct_tiles: s.contents,
            bytes: s.bytes,
        });
        Ok(())
    };
    finish(idx_writer, None, TileType::Png, None)?;
    for (look, w) in looks.iter().zip(look_writers) {
        finish(w, Some(&look.name), TileType::Webp, Some(enc))?;
    }
    Ok(())
}

/// Equirectangular width for a raster's globe texture (WebP is limited to 16383 px).
pub fn globe_width(max_level: u32) -> usize {
    let wanted = (rmap::TILE as f64 * (1u64 << max_level) as f64 * 4.0) as usize;
    wanted.next_power_of_two().clamp(1024, 8192)
}

/// Equirectangular class-index image (`w × w/2`, row 0 at 90° N, column 0 at 180° W).
pub fn equirect(raster: &Raster, w: usize) -> Result<Vec<u8>> {
    let h = w / 2;
    let level = level_for(2.0 * std::f64::consts::PI / w as f64, raster.max_level);
    let mut img = vec![0u8; w * h];
    img.par_chunks_mut(w * 16).enumerate().try_for_each_init(
        || TileCache::new(raster, 1024),
        |cache, (band, rows)| -> Result<()> {
            for (k, row) in rows.chunks_mut(w).enumerate() {
                let i = band * 16 + k;
                let lat = 90.0 - (i as f64 + 0.5) * 180.0 / h as f64;
                cache.sample_row(level, lat, -180.0, 360.0 / w as f64, row)?;
            }
            Ok(())
        },
    )?;
    Ok(img)
}

/// Palettes of every LOOK of every raster: what a client needs to recolour index tiles.
pub fn palettes(rasters: &[Raster]) -> Value {
    let mut m = serde_json::Map::new();
    for r in rasters {
        let mut looks = serde_json::Map::new();
        for l in &r.looks {
            let colours: Vec<String> = l.values.iter().map(|v| format!("#{:02x}{:02x}{:02x}", v.rgb[0], v.rgb[1], v.rgb[2])).collect();
            let shade: Vec<u8> = l.values.iter().map(|v| v.shade).collect();
            let kind: Vec<u8> = l.values.iter().map(|v| v.kind).collect();
            let legend: Vec<String> = l.legend.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect();
            looks.insert(l.name.clone(), json!({ "colours": colours, "shade": shade, "pixel_class": kind, "legend": legend }));
        }
        m.insert(r.name.clone(), json!({ "max_level": r.max_level, "min_altitude_km": r.display_scale, "looks": looks }));
    }
    Value::Object(m)
}

pub fn run(rasters: &[Raster], styles: &[MapStyle], out: &Path, only: Option<&[String]>, log: &dyn Fn(String)) -> Result<RasterReport> {
    let t0 = std::time::Instant::now();
    let mut report = RasterReport::default();
    let used = used_looks(styles);
    std::fs::create_dir_all(out.join("raster"))?;
    std::fs::write(out.join("raster/palettes.json"), serde_json::to_vec_pretty(&palettes(rasters))?)?;
    for r in rasters {
        let names = used.get(&r.name.to_ascii_uppercase()).cloned().unwrap_or_default();
        for l in &r.looks {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&l.name)) {
                report.unused_looks.push((r.name.clone(), l.name.clone()));
            }
        }
        if only.is_some_and(|o| !o.iter().any(|n| n.eq_ignore_ascii_case(&r.name))) {
            continue;
        }
        let looks: Vec<&Look> = names.iter().filter_map(|n| r.look(n)).collect();
        let t = std::time::Instant::now();
        log(format!("raster {} (level {}, looks {:?})", r.name, r.max_level, looks.iter().map(|l| &l.name).collect::<Vec<_>>()));
        raster_archives(r, &looks, out, &mut report, log).with_context(|| format!("raster {}", r.name))?;
        // Globe textures (also cover the polar caps beyond Web Mercator's ±85.05°).
        let w = globe_width(r.max_level);
        let img = equirect(r, w)?;
        let dir = out.join("raster").join(r.name.to_ascii_lowercase());
        let p = dir.join("index.equirect.png");
        std::fs::write(&p, png_gray(&img, w as u32, (w / 2) as u32))?;
        report.globe_textures.push(format!("raster/{}/index.equirect.png", r.name.to_ascii_lowercase()));
        for look in &looks {
            let rgb: Vec<u8> = img.iter().flat_map(|&v| look.values[v as usize].rgb).collect();
            let bytes = if encoding_for(&r.name) == Encoding::Lossy { webp_lossy(&rgb, w as u32, (w / 2) as u32) } else { webp_lossless(&rgb, w as u32, (w / 2) as u32) };
            let name = format!("{}.equirect.webp", look.name.to_ascii_lowercase());
            std::fs::write(dir.join(&name), bytes)?;
            report.globe_textures.push(format!("raster/{}/{name}", r.name.to_ascii_lowercase()));
        }
        log(format!("  {} done in {:.1} s (globe texture {w}×{})", r.name, t.elapsed().as_secs_f64(), w / 2));
    }
    report.seconds = t0.elapsed().as_secs_f64();
    Ok(report)
}

/// Agreement between a max-zoom index archive and direct octahedral lookups at the finest level,
/// over `samples` pseudo-random pixels at latitudes where the finest level is used.
pub fn verify_index(raster: &Raster, archive: &pmtiles::Reader, samples: usize) -> Result<(usize, usize)> {
    let maxz = archive.header.max_zoom;
    let n = (1u64 << maxz) as f64 * TILE_SIZE as f64;
    let mut cache = TileCache::new(raster, 256);
    let side = rmap::image_side(raster.max_level) as u32;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let (mut same, mut total) = (0, 0);
    let mut tiles: HashMap<(u32, u32), Vec<u8>> = HashMap::new();
    while total < samples {
        let px = (next() % n as u64) as f64;
        let py = (next() % n as u64) as f64;
        let lat = geo::y_to_lat((py + 0.5) / n);
        let res = 2.0 * std::f64::consts::PI * lat.to_radians().cos() / n;
        if level_for(res, raster.max_level) != raster.max_level {
            continue;
        }
        let lon = (px + 0.5) / n * 360.0 - 180.0;
        let (tx, ty) = ((px as usize / TILE_SIZE) as u32, (py as usize / TILE_SIZE) as u32);
        let tile = match tiles.get(&(tx, ty)) {
            Some(t) => t.clone(),
            None => {
                let bytes = archive.get(maxz, tx, ty)?.context("missing tile")?;
                let t = decode_png_gray(&bytes)?;
                tiles.insert((tx, ty), t.clone());
                t
            }
        };
        let v = tile[(py as usize % TILE_SIZE) * TILE_SIZE + px as usize % TILE_SIZE];
        let (r, c) = encarta_formats::mad::octahedral::pixel_of(lat, lon, side);
        if cache.pixel(raster.max_level, r as usize, c as usize)? == v {
            same += 1;
        }
        total += 1;
    }
    Ok((same, total))
}

pub fn decode_png_gray(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes)).read_info()?;
    let mut buf = vec![0u8; d.output_buffer_size().context("png size")?];
    let info = d.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On the user's disc (skipped without `extracted/`): HYPSO ocean value 17 at 0° N 140° W
    /// (mad.md §5.4) in the Mercator tile, and the PMTiles round trip of real tiles.
    #[test]
    fn hypso_tile_from_disc() {
        let root = std::env::var_os("ENCARTA_EXTRACTED")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"));
        let Ok(atlas) = encarta_formats::mad::Atlas::open_edition(&root, encarta_formats::mad::Edition::World) else {
            eprintln!("skipped: no extracted disc at {}", root.display());
            return;
        };
        let terrain = atlas.terrain().unwrap();
        let hypso = terrain.raster("HYPSO").unwrap();
        let mut cache = TileCache::new(hypso, 64);
        let z = 6u8;
        let n = 64.0 * TILE_SIZE as f64;
        let (px, py) = (geo::lon_to_x(-140.0) * n, geo::lat_to_y(0.3) * n);
        let (tx, ty) = ((px / 256.0) as u32, (py / 256.0) as u32);
        let t = cache.mercator_tile(z, tx, ty, hypso.max_level).unwrap();
        assert_eq!(t[(py as usize % 256) * 256 + px as usize % 256], 17);
        let dir = std::env::temp_dir().join(format!("atlas-raster-test-{}", std::process::id()));
        let path = dir.join("t.pmtiles");
        let mut w = Writer::create(&path).unwrap();
        let png = png_gray(&t, 256, 256);
        w.add(z, tx, ty, &png).unwrap();
        let info = Info { tile_type: TileType::Png, gzip_tiles: false, min_zoom: z, max_zoom: z, bounds: (-180.0, -85.0, 180.0, 85.0), center: (0.0, 0.0, z) };
        w.finish(&info, &json!({})).unwrap();
        let r = pmtiles::Reader::open(&path).unwrap();
        assert_eq!(decode_png_gray(&r.get(z, tx, ty).unwrap().unwrap()).unwrap(), t);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zoom_and_level_choice() {
        // Mercator z9 at the equator matches octahedral level 8 (HYPSO), z6 level 5 (SATELLITE).
        assert_eq!(max_zoom(8), 9);
        assert_eq!(max_zoom(5), 6);
        assert_eq!(max_zoom(0), 1);
        let res = |z: u32| 2.0 * std::f64::consts::PI / (256.0 * (1u64 << z) as f64);
        assert_eq!(level_for(res(9), 8), 8);
        assert_eq!(level_for(res(0), 8), 0);
        assert_eq!(level_for(res(5), 8), 4);
        // At 60° the ground pixel is half as large: one level finer.
        assert_eq!(level_for(res(5) * 0.5, 8), 5);
    }

    #[test]
    fn row_interpolation_matches_exact_projection() {
        // The interpolated row must hit the same image pixel as the exact inverse projection
        // almost everywhere (both are evaluated on a synthetic large image).
        let side = rmap::image_side(8) as f64;
        let h = side / 2.0;
        for &lat in &[-70.0, -12.3, 0.4, 33.3, 65.0, 84.9] {
            let w = 256;
            let (lon0, dlon) = (-0.3, 0.0027);
            let mut mismatches = 0;
            let mut i = 0;
            while i < w {
                let j = (i + 8).min(w - 1);
                let (la, lb) = (lon0 + (i as f64 + 0.5) * dlon, lon0 + (j as f64 + 0.5) * dlon);
                let (ua, va) = from_lat_lon(lat, la, 1.0);
                let (ub, vb) = from_lat_lon(lat, lb, 1.0);
                for k in i..j {
                    let t = (k - i) as f64 / (j - i) as f64;
                    let (u, v) = from_lat_lon(lat, lon0 + (k as f64 + 0.5) * dlon, 1.0);
                    let same_quadrant = ((la + 180.0) / 90.0).floor() == ((lb + 180.0) / 90.0).floor();
                    if same_quadrant {
                        let (ui, vi) = (ua + (ub - ua) * t, va + (vb - va) * t);
                        if ((ui * h + h).floor() != (u * h + h).floor()) || ((vi * h + h).floor() != (v * h + h).floor()) {
                            mismatches += 1;
                        }
                    }
                }
                if j == w - 1 {
                    break;
                }
                i = j;
            }
            assert!(mismatches <= 2, "lat {lat}: {mismatches} pixels differ");
        }
    }
}
