//! The octahedral projection of the raster images (mad.md §6).
//!
//! Image coordinates are relative to the image centre: `dx` to the right, `dy` downwards, both in
//! `[-h, h]` where `h` is half the image side. The north pole is at the centre, the south pole at
//! the four corners, and the equator is the diamond through the edge midpoints:
//! right = 90° E, top = 180°, left = 90° W, bottom = 0°.
//!
//! Inside each of the eight triangles, distance from the pole is linear along the two triangle
//! edges, and a line of constant distance is the great-circle arc between the two edge points.

use std::f64::consts::FRAC_PI_2;

type V3 = [f64; 3];

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn scale_add(a: V3, sa: f64, b: V3, sb: f64) -> V3 {
    [a[0] * sa + b[0] * sb, a[1] * sa + b[1] * sb, a[2] * sa + b[2] * sb]
}
fn angle(a: V3, b: V3) -> f64 {
    norm(cross(a, b)).atan2(dot(a, b))
}

/// Constant-speed great-circle interpolation between unit vectors.
pub fn slerp(p: V3, q: V3, t: f64) -> V3 {
    let omega = angle(p, q);
    if omega < 1e-12 {
        return p;
    }
    let s = omega.sin();
    scale_add(p, ((1.0 - t) * omega).sin() / s, q, (t * omega).sin() / s)
}

fn equator(lon_deg: f64) -> V3 {
    let l = lon_deg.to_radians();
    [l.cos(), l.sin(), 0.0]
}

/// Longitudes of the face's equator vertices: `A` on the horizontal axis, `B` on the vertical.
fn face_of(dx: f64, dy: f64) -> (f64, f64) {
    (if dx >= 0.0 { 90.0 } else { -90.0 }, if dy < 0.0 { 180.0 } else { 0.0 })
}

/// Unit vector of image point `(dx, dy)` (relative to the centre) in an image of half-side `h`.
pub fn to_vector(dx: f64, dy: f64, h: f64) -> V3 {
    let (a_lon, b_lon) = face_of(dx, dy);
    let (ax, ay) = (dx.abs(), dy.abs());
    let (pole, r, s) = if ax + ay <= h {
        let sum = ax + ay;
        ([0.0, 0.0, 1.0], sum / h, if sum > 0.0 { ay / sum } else { 0.0 })
    } else {
        let (ex, ey) = (h - ax, h - ay);
        let sum = ex + ey;
        ([0.0, 0.0, -1.0], sum / h, if sum > 0.0 { ex / sum } else { 0.0 })
    };
    let a = slerp(pole, equator(a_lon), r);
    let b = slerp(pole, equator(b_lon), r);
    slerp(a, b, s)
}

/// Latitude and longitude in degrees of image point `(dx, dy)`.
pub fn to_lat_lon(dx: f64, dy: f64, h: f64) -> (f64, f64) {
    let v = to_vector(dx, dy, h);
    vector_lat_lon(v)
}

pub fn vector_lat_lon(v: V3) -> (f64, f64) {
    let lat = v[2].clamp(-1.0, 1.0).asin().to_degrees();
    let lon = if v[0].abs() < 1e-15 && v[1].abs() < 1e-15 { 0.0 } else { v[1].atan2(v[0]).to_degrees() };
    (lat, lon)
}

pub fn lat_lon_vector(lat: f64, lon: f64) -> V3 {
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    [la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()]
}

/// Image point `(dx, dy)` relative to the centre of the point at `lat`, `lon` (degrees).
///
/// The distance from the pole along the face edges has a closed form: for a point `v` and the
/// face's pole `P`, edge vertices `A`, `B`, the arc at angular distance θ from the pole passes
/// through `v` exactly when `tan θ = (v·A + v·B) / (v·P)`.
pub fn from_lat_lon(lat: f64, lon: f64, h: f64) -> (f64, f64) {
    let v = lat_lon_vector(lat, lon);
    let lon = ((lon + 180.0).rem_euclid(360.0)) - 180.0;
    let (a_lon, sx) = if lon >= 0.0 { (90.0, 1.0) } else { (-90.0, -1.0) };
    let (b_lon, sy) = if lon.abs() > 90.0 { (180.0, -1.0) } else { (0.0, 1.0) };
    let north = lat >= 0.0;
    let pole: V3 = if north { [0.0, 0.0, 1.0] } else { [0.0, 0.0, -1.0] };
    let (a_dir, b_dir) = (equator(a_lon), equator(b_lon));
    let along = dot(v, a_dir).max(0.0) + dot(v, b_dir).max(0.0);
    let theta = along.atan2(dot(v, pole).max(0.0));
    let r = (theta / FRAC_PI_2).clamp(0.0, 1.0);
    let a = slerp(pole, a_dir, r);
    let b = slerp(pole, b_dir, r);
    let ab = angle(a, b);
    let s = if ab > 1e-12 { (angle(a, v) / ab).clamp(0.0, 1.0) } else { 0.0 };
    let (ax, ay) = if north { ((1.0 - s) * r * h, s * r * h) } else { (h - s * r * h, h - (1.0 - s) * r * h) };
    (sx * ax, sy * ay)
}

/// Centre of pixel `(row, col)` relative to the image centre, for an image of side `2h`.
pub fn pixel_center(row: u32, col: u32, h: f64) -> (f64, f64) {
    (col as f64 + 0.5 - h, row as f64 + 0.5 - h)
}

/// The pixel `(row, col)` containing the point at `lat`, `lon` in an image of side `side`.
pub fn pixel_of(lat: f64, lon: f64, side: u32) -> (u32, u32) {
    let h = side as f64 / 2.0;
    let (dx, dy) = from_lat_lon(lat, lon, h);
    let clamp = |v: f64| (v + h).floor().clamp(0.0, side as f64 - 1.0) as u32;
    (clamp(dy), clamp(dx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64), eps: f64) -> bool {
        (a.0 - b.0).abs() < eps && (a.1 - b.1).abs() < eps
    }

    #[test]
    fn fixed_points() {
        let h = 100.0;
        assert!((to_lat_lon(0.0, 0.0, h).0 - 90.0).abs() < 1e-9);
        for (dx, dy) in [(h, h), (-h, h), (h, -h), (-h, -h)] {
            assert!((to_lat_lon(dx, dy, h).0 + 90.0).abs() < 1e-9);
        }
        assert!(close(to_lat_lon(h, 0.0, h), (0.0, 90.0), 1e-9));
        assert!(close(to_lat_lon(-h, 0.0, h), (0.0, -90.0), 1e-9));
        assert!(close(to_lat_lon(0.0, h, h), (0.0, 0.0), 1e-9));
        let (lat, lon) = to_lat_lon(0.0, -h, h);
        assert!(lat.abs() < 1e-9 && (lon.abs() - 180.0).abs() < 1e-9);
        // Halfway from the centre to the right vertex: 45° N on the 90° E meridian.
        assert!(close(to_lat_lon(h / 2.0, 0.0, h), (45.0, 90.0), 1e-9));
        // Halfway along the right edge towards the bottom-right corner: 45° S, 90° E.
        assert!(close(to_lat_lon(h, h / 2.0, h), (-45.0, 90.0), 1e-9));
        // Midpoint of the north-east-of-Greenwich face edge: equator at 45° E.
        assert!(close(to_lat_lon(h / 2.0, h / 2.0, h), (0.0, 45.0), 1e-9));
    }

    #[test]
    fn round_trips() {
        let h = 1000.0;
        let mut worst: f64 = 0.0;
        for i in 0..=80 {
            for j in 0..=80 {
                let (dx, dy) = (-h + i as f64 * 25.0, -h + j as f64 * 25.0);
                let (lat, lon) = to_lat_lon(dx, dy, h);
                let (bx, by) = from_lat_lon(lat, lon, h);
                // Points on the boundaries can legitimately map to an equivalent image point
                // (the four corners are all the south pole, the top edge is ±180°).
                let back = to_lat_lon(bx, by, h);
                let v0 = lat_lon_vector(lat, lon);
                let v1 = lat_lon_vector(back.0, back.1);
                assert!(angle(v0, v1) < 1e-9, "{dx},{dy}");
                if dx.abs() < h && dy.abs() < h && dy > -h + 1.0 {
                    worst = worst.max((bx - dx).abs()).max((by - dy).abs());
                }
            }
        }
        assert!(worst < 1e-6, "round trip error {worst}");
    }

    #[test]
    fn rows_bulge_towards_the_pole() {
        // Points with r = 1/2 lie on the great circle through the two edge points at r = 1/2,
        // spaced evenly by s.
        let h = 1.0;
        let mid = to_vector(0.25, 0.25, h);
        let a = to_vector(0.5, 0.0, h);
        let b = to_vector(0.0, 0.5, h);
        let n = cross(a, b);
        assert!(dot(mid, n).abs() < 1e-12, "midpoint lies on the great circle through the edge points");
        assert!((angle(a, mid) - angle(mid, b)).abs() < 1e-12);
    }

    #[test]
    fn pixel_lookup() {
        let side = 254;
        assert_eq!(pixel_of(90.0, 0.0, side), (127, 127));
        assert_eq!(pixel_of(-89.9999, 45.0, side), (253, 253));
        assert_eq!(pixel_of(0.0, 90.0, side).1, 253);
        let (r, c) = pixel_of(10.0, -30.0, side);
        let (dx, dy) = pixel_center(r, c, 127.0);
        let (lat, lon) = to_lat_lon(dx, dy, 127.0);
        assert!((lat - 10.0).abs() < 1.0 && (lon + 30.0).abs() < 1.0);
    }
}
