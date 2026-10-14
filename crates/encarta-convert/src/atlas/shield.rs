//! Road-shield sprites (mad-styling.md §8.1): the ten shield shapes of GeoLabel byte 12, drawn
//! here from scratch (the shapes of the original are not copied), as stretchable sprites for
//! MapLibre's `icon-text-fit: both`. A sprite is a 24 × 24 unit box whose middle row and column
//! (units 11–13) stretch; the text box maps onto the `content` rectangle.

use serde_json::{Value, json};

/// Shape and colours of a shield: GeoLabel bytes 12, 13–15 and 16, 17–19, 20–22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Shield {
    pub shape: u8,
    pub outline: [u8; 3],
    pub width: u8,
    pub fill: [u8; 3],
    pub fill2: [u8; 3],
}

impl Shield {
    /// Sprite name, unique per shape and colours.
    pub fn name(&self) -> String {
        let h = |c: [u8; 3]| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        format!("shield{}-{}-{}-{}-{}", self.shape, h(self.outline), self.width, h(self.fill), h(self.fill2))
    }
}

const BOX: f64 = 24.0;

/// Polygon outlines in the unit square (our own simple drawings).
const INTERSTATE: &[(f64, f64)] = &[(0.02, 0.10), (0.25, 0.0), (0.5, 0.08), (0.75, 0.0), (0.98, 0.10), (0.98, 0.55), (0.5, 1.0), (0.02, 0.55)];
const US_HIGHWAY: &[(f64, f64)] = &[(0.15, 0.0), (0.85, 0.0), (1.0, 0.2), (1.0, 0.6), (0.5, 1.0), (0.0, 0.6), (0.0, 0.2)];
const MAPLE: &[(f64, f64)] = &[
    (0.5, 0.0), (0.62, 0.2), (0.72, 0.15), (0.70, 0.38), (0.88, 0.30), (0.84, 0.45), (1.0, 0.50), (0.80, 0.68), (0.84, 0.76),
    (0.56, 0.72), (0.56, 1.0), (0.44, 1.0), (0.44, 0.72), (0.16, 0.76), (0.20, 0.68), (0.0, 0.50), (0.16, 0.45), (0.12, 0.30),
    (0.30, 0.38), (0.28, 0.15), (0.38, 0.2),
];
const MEXICO: &[(f64, f64)] = &[(0.0, 0.3), (0.1, 0.1), (0.3, 0.02), (0.5, 0.0), (0.7, 0.02), (0.9, 0.1), (1.0, 0.3), (1.0, 0.55), (0.5, 1.0), (0.0, 0.55)];

fn in_polygon(poly: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Is the unit point inside the shape?
fn inside(shape: u8, x: f64, y: f64) -> bool {
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return false;
    }
    match shape {
        1 | 2 => true,
        3 | 4 => {
            // Rounded corners of radius 6 units.
            let r = 6.0 / BOX;
            let (cx, cy) = (x.clamp(r, 1.0 - r), y.clamp(r, 1.0 - r));
            (x - cx).hypot(y - cy) <= r
        }
        5 | 6 => (x - 0.5).powi(2) + (y - 0.5).powi(2) <= 0.25,
        7 => in_polygon(INTERSTATE, x, y),
        8 => in_polygon(US_HIGHWAY, x, y),
        9 => in_polygon(MAPLE, x, y),
        10 => in_polygon(MEXICO, x, y),
        _ => false,
    }
}

/// Is the second fill part (shapes 7 and 10: the top band) at this point?
fn part2(shape: u8, y: f64) -> bool {
    matches!(shape, 7 | 10) && y < 8.0 / BOX
}

/// Text rectangle `[left, top, right, bottom]` in box units for each shape.
fn content(shape: u8) -> [f64; 4] {
    match shape {
        1 => [2.0, 2.0, 22.0, 22.0],
        2 => [4.0, 4.0, 20.0, 20.0],
        3 => [4.0, 2.0, 20.0, 22.0],
        4 => [5.0, 4.0, 19.0, 20.0],
        5 => [6.0, 4.0, 18.0, 20.0],
        6 => [7.0, 5.0, 17.0, 19.0],
        7 => [4.0, 9.0, 20.0, 18.0],
        8 => [4.0, 4.0, 20.0, 17.0],
        9 => [5.0, 8.0, 19.0, 16.0],
        _ => [4.0, 9.0, 20.0, 17.0],
    }
}

const DIRS: usize = 16;

/// Is the whole disc of `radius` units around the point inside (sampled on a ring)?
fn eroded(shape: u8, x: f64, y: f64, radius: f64) -> bool {
    if !inside(shape, x, y) {
        return false;
    }
    (0..DIRS).all(|k| {
        let a = k as f64 * std::f64::consts::TAU / DIRS as f64;
        inside(shape, x + radius / BOX * a.cos(), y + radius / BOX * a.sin())
    })
}

/// Renders the sprite at `ratio` pixels per unit: `(size in px, rgba, sprite JSON fields stretchX, stretchY, content)`.
pub fn render(s: &Shield, ratio: usize) -> (usize, Vec<u8>, Value) {
    let n = BOX as usize * ratio;
    let mut img = vec![0u8; n * n * 4];
    let w = s.width as f64;
    let inset = matches!(s.shape, 2 | 4 | 6);
    const SS: usize = 4;
    for py in 0..n {
        for px in 0..n {
            let mut acc = [0f64; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f64 + (sx as f64 + 0.5) / SS as f64) / n as f64;
                    let y = (py as f64 + (sy as f64 + 0.5) / SS as f64) / n as f64;
                    if !inside(s.shape, x, y) {
                        continue;
                    }
                    let ring = if w <= 0.0 {
                        false
                    } else if inset {
                        eroded(s.shape, x, y, 1.5) && !eroded(s.shape, x, y, 1.5 + w)
                    } else {
                        !eroded(s.shape, x, y, w)
                    };
                    let c = if ring { s.outline } else if part2(s.shape, y) { s.fill2 } else { s.fill };
                    for k in 0..3 {
                        acc[k] += c[k] as f64;
                    }
                    acc[3] += 1.0;
                }
            }
            if acc[3] > 0.0 {
                let d = (py * n + px) * 4;
                for k in 0..3 {
                    img[d + k] = (acc[k] / acc[3]).round() as u8;
                }
                img[d + 3] = (acc[3] / (SS * SS) as f64 * 255.0).round() as u8;
            }
        }
    }
    let r = ratio as f64;
    let c = content(s.shape);
    let meta = json!({
        "stretchX": [[11.0 * r, 13.0 * r]],
        "stretchY": [[11.0 * r, 13.0 * r]],
        "content": [c[0] * r, c[1] * r, c[2] * r, c[3] * r],
    });
    (n, img, meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shield(shape: u8) -> Shield {
        Shield { shape, outline: [255, 255, 255], width: 1, fill: [0, 0, 255], fill2: [255, 0, 0] }
    }

    #[test]
    fn every_shape_draws_an_outline_and_fill() {
        for shape in 1..=10 {
            let (n, img, meta) = render(&shield(shape), 2);
            assert_eq!(img.len(), n * n * 4);
            assert!(img.chunks(4).any(|p| p[3] == 255 && p[..3] == [255, 255, 255]), "shape {shape} outline");
            assert!(img.chunks(4).any(|p| p[3] == 255 && p[..3] == [0, 0, 255]), "shape {shape} fill");
            assert_eq!(meta["stretchX"][0][0], 22.0);
        }
    }

    #[test]
    fn names_differ_by_colour() {
        let mut b = shield(1);
        b.fill = [1, 2, 3];
        assert_ne!(shield(1).name(), b.name());
    }
}
