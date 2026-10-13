//! Validation previews: mosaics read back from the written PMTiles (raster tiles decoded, MVT
//! features drawn on top) → `out/report/atlas-preview/*.png`.

use super::geo;
use super::mvt;
use super::pmtiles::Reader;
use anyhow::{Context, Result};
use std::path::Path;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, rgba: vec![40; w * h * 4] }
    }

    fn put(&mut self, x: i64, y: i64, c: [u8; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        for (d, &s) in self.rgba[i..i + 3].iter_mut().zip(&c) {
            *d = (*d as f32 * (1.0 - alpha) + s as f32 * alpha) as u8;
        }
        self.rgba[i + 3] = 255;
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), c: [u8; 3], alpha: f32) {
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).clamp(1, 100_000);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            self.put((a.0 + (b.0 - a.0) * t) as i64, (a.1 + (b.1 - a.1) * t) as i64, c, alpha);
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let f = std::fs::File::create(path)?;
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), self.w as u32, self.h as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()?.write_image_data(&self.rgba)?;
        Ok(())
    }
}

fn decode_image(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    if bytes.starts_with(b"\x89PNG") {
        let g = super::raster::decode_png_gray(bytes)?;
        let side = (g.len() as f64).sqrt() as usize;
        return Ok((side, side, g.iter().flat_map(|&v| [v, v, v, 255]).collect()));
    }
    let img = webp::Decoder::new(bytes).decode().context("webp decode")?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let data: &[u8] = &img;
    let rgba = if img.is_alpha() { data.to_vec() } else { data.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
    Ok((w, h, rgba))
}

/// Tile range `(x0, y0, x1, y1)` covering a bbox at zoom `z`.
fn tile_range(z: u8, bbox: (f64, f64, f64, f64)) -> (u32, u32, u32, u32) {
    let n = (1u32 << z) as f64;
    let (w, s, e, north) = bbox;
    let x0 = (geo::lon_to_x(w) * n).floor().max(0.0) as u32;
    let x1 = ((geo::lon_to_x(e) * n).ceil() as u32).min(1 << z).max(x0 + 1) - 1;
    let y0 = (geo::lat_to_y(north) * n).floor().max(0.0) as u32;
    let y1 = ((geo::lat_to_y(s) * n).ceil() as u32).min(1 << z).max(y0 + 1) - 1;
    (x0, y0, x1, y1)
}

fn class_colour(layer: &str, class_name: &str) -> Option<([u8; 3], f32)> {
    let n = class_name.to_ascii_lowercase();
    Some(match layer {
        "lines" if n.contains("sovereign") => ([220, 30, 30], 1.0),
        "lines" if n.contains("admin") => ([200, 80, 160], 0.8),
        "lines" if n.contains("coast") => ([0, 0, 0], 1.0),
        "lines" if n.contains("river") || n.contains("canal") => ([30, 90, 230], 0.8),
        "lines" if n.contains("road") || n.contains("highway") || n.contains("motorway") || n.contains("arterial") => ([150, 80, 20], 0.9),
        "lines" if n.contains("rail") => ([60, 60, 60], 0.8),
        "lines" if n.contains("graticule") => return None,
        "lines" => ([90, 90, 90], 0.5),
        "areas" if n == "ocean" => ([0, 60, 200], 0.6),
        "areas" if n.starts_with("lake") => ([0, 140, 255], 0.7),
        "areas" if n.contains("urban") || n.contains("city limits") => ([255, 140, 0], 0.8),
        "areas" => return None,
        "points" => ([255, 255, 0], 1.0),
        "labels" => ([255, 0, 255], 1.0),
        _ => return None,
    })
}

/// Raster mosaic at zoom `z` over `bbox` with vector archives drawn on top.
pub fn render(raster: Option<&Reader>, vectors: &[&Reader], z: u8, bbox: (f64, f64, f64, f64), only_layers: &[&str]) -> Result<Canvas> {
    let (x0, y0, x1, y1) = tile_range(z, bbox);
    let (tw, th) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
    let mut c = Canvas::new(tw * 256, th * 256);
    if let Some(r) = raster {
        let rz = z.min(r.header.max_zoom);
        let k = 1u32 << (z - rz);
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let Some(bytes) = r.get(rz, tx / k, ty / k)? else { continue };
                let (w, h, px) = decode_image(&bytes)?;
                let (ox, oy) = ((tx % k) as usize * w / k as usize, (ty % k) as usize * h / k as usize);
                for y in 0..256 {
                    for x in 0..256 {
                        let (sx, sy) = (ox + x * w / (256 * k as usize), oy + y * h / (256 * k as usize));
                        let s = (sy * w + sx) * 4;
                        let d = (((ty - y0) as usize * 256 + y) * c.w + (tx - x0) as usize * 256 + x) * 4;
                        c.rgba[d..d + 4].copy_from_slice(&px[s..s + 4]);
                    }
                }
            }
        }
    }
    for v in vectors {
        let vz = z.min(v.header.max_zoom);
        if vz < v.header.min_zoom {
            continue;
        }
        let k = 1u32 << (z - vz);
        let scale = 256.0 * k as f64 / mvt::EXTENT as f64;
        let (vx0, vy0, vx1, vy1) = (x0 / k, y0 / k, x1 / k, y1 / k);
        for vy in vy0..=vy1 {
            for vx in vx0..=vx1 {
                let Some(raw) = v.get(vz, vx, vy)? else { continue };
                let Some(layers) = mvt::decode_tile(&raw) else { continue };
                let ox = (vx as f64 * k as f64 - x0 as f64) * 256.0;
                let oy = (vy as f64 * k as f64 - y0 as f64) * 256.0;
                let to = |p: (i32, i32)| (ox + p.0 as f64 * scale, oy + p.1 as f64 * scale);
                for l in &layers {
                    if !only_layers.is_empty() && !only_layers.contains(&l.name.as_str()) {
                        continue;
                    }
                    for f in &l.features {
                        let cname = f.props.iter().find(|p| p.0 == "class_name").map(|p| match &p.1 {
                            mvt::Value::Str(s) => s.clone(),
                            _ => String::new(),
                        });
                        let Some((col, a)) = class_colour(&l.name, cname.as_deref().unwrap_or("")) else { continue };
                        for part in &f.parts {
                            match f.geom_type {
                                mvt::POINT => {
                                    for &p in part {
                                        let (x, y) = to(p);
                                        for d in -1..=1 {
                                            c.put(x as i64 + d, y as i64, col, a);
                                            c.put(x as i64, y as i64 + d, col, a);
                                        }
                                    }
                                }
                                _ => {
                                    for w in part.windows(2) {
                                        c.line(to(w[0]), to(w[1]), col, a);
                                    }
                                    if f.geom_type == mvt::POLYGON && part.len() > 2 {
                                        c.line(to(*part.last().unwrap()), to(part[0]), col, a);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(c)
}

pub struct Preview {
    pub name: &'static str,
    pub raster: Option<&'static str>,
    pub vectors: &'static [&'static str],
    pub zoom: u8,
    pub bbox: (f64, f64, f64, f64),
    pub layers: &'static [&'static str],
}

pub const PREVIEWS: &[Preview] = &[
    Preview { name: "world-hypso-z2", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 2, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &["lines"] },
    Preview { name: "europe-hypso-z5", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 5, bbox: (-10.0, 36.0, 25.0, 58.0), layers: &["lines", "points"] },
    Preview { name: "alps-hypso-z9", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &[], zoom: 9, bbox: (7.0, 45.6, 8.4, 46.4), layers: &[] },
    Preview { name: "alps-political-z8", raster: Some("raster/hypso/political.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 8, bbox: (6.0, 45.5, 10.5, 47.5), layers: &["lines", "areas"] },
    Preview { name: "denmark-hypso-z7-areas", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 7, bbox: (7.5, 54.3, 13.0, 57.8), layers: &["areas", "lines"] },
    Preview { name: "antimeridian-z5", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 4, bbox: (-180.0, -30.0, 180.0, 75.0), layers: &["lines"] },
    Preview { name: "world-satellite-z3", raster: Some("raster/satellite/satellite.pmtiles"), vectors: &["vector/earth.pmtiles"], zoom: 3, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &["lines"] },
    Preview { name: "world-night-z2", raster: Some("raster/night/night.pmtiles"), vectors: &[], zoom: 2, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &[] },
    Preview { name: "world-tectonic-z2", raster: Some("raster/tectonic/tectonic.pmtiles"), vectors: &[], zoom: 2, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &[] },
    Preview { name: "moon-z2", raster: Some("raster/moon/moon.pmtiles"), vectors: &["vector/moon.pmtiles"], zoom: 2, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &[] },
    Preview { name: "paris-detail-z12", raster: Some("raster/hypso/hypso.pmtiles"), vectors: &["vector/earth.pmtiles", "vector/earth-detail.pmtiles"], zoom: 11, bbox: (2.1, 48.7, 2.6, 49.0), layers: &[] },
    Preview { name: "hypso-index-z3", raster: Some("raster/hypso/index.pmtiles"), vectors: &[], zoom: 3, bbox: (-180.0, -85.0, 180.0, 85.0), layers: &[] },
];

pub fn run(out: &Path, report_dir: &Path, log: &dyn Fn(String)) -> Result<Vec<String>> {
    let mut written = Vec::new();
    let mut cache: std::collections::HashMap<&str, Reader> = std::collections::HashMap::new();
    for p in PREVIEWS {
        let mut names: Vec<&str> = p.vectors.to_vec();
        names.extend(p.raster);
        let mut missing = false;
        for n in &names {
            if !cache.contains_key(n) {
                match Reader::open(&out.join(n)) {
                    Ok(r) => {
                        cache.insert(n, r);
                    }
                    Err(_) => missing = true,
                }
            }
        }
        if missing {
            log(format!("  preview {}: skipped (archive missing)", p.name));
            continue;
        }
        let raster = p.raster.map(|r| &cache[r]);
        let vectors: Vec<&Reader> = p.vectors.iter().map(|v| &cache[v]).collect();
        let c = render(raster, &vectors, p.zoom, p.bbox, p.layers)?;
        let path = report_dir.join(format!("atlas-preview/{}.png", p.name));
        c.save(&path)?;
        written.push(format!("report/atlas-preview/{}.png", p.name));
    }
    log(format!("  {} previews", written.len()));
    Ok(written)
}
