//! Web Mercator helpers and the altitude → zoom conversion.
//!
//! World coordinates are in `[0, 1]²` (x east, y south). Tile `(z, x, y)` covers
//! `[x, x+1] / 2^z × [y, y+1] / 2^z`.

use std::f64::consts::PI;

/// Latitude limit of Web Mercator.
pub const MAX_LAT: f64 = 85.051_128_779_806_59;

pub fn lon_to_x(lon: f64) -> f64 {
    (lon + 180.0) / 360.0
}

/// Latitude → world y (unclamped; ±90° are clamped to ±89.9999° to stay finite).
pub fn lat_to_y(lat: f64) -> f64 {
    let l = lat.clamp(-89.9999, 89.9999).to_radians();
    (1.0 - (l.tan() + 1.0 / l.cos()).ln() / PI) / 2.0
}

pub fn y_to_lat(y: f64) -> f64 {
    (PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees()
}

#[cfg(test)]
pub fn x_to_lon(x: f64) -> f64 {
    x * 360.0 - 180.0
}

/// Bounds `(west, south, east, north)` of a tile in degrees.
#[cfg(test)]
pub fn tile_bounds(z: u8, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = (1u64 << z) as f64;
    (x_to_lon(x as f64 / n), y_to_lat((y + 1) as f64 / n), x_to_lon((x + 1) as f64 / n), y_to_lat(y as f64 / n))
}

/// Altitude → zoom: the single place where Encarta's camera altitudes become MapLibre zooms
/// (mad-styling.md §2.2). For 512-px tiles at the equator,
/// `zoom(a) = log2(40075.017 · (P/m) / (512 · 2.5 · a))`. `P/m` (the view constants of
/// mad-styling.md §2.1) was measured in the reference VM (docs/vm-findings.md, V1/V2): 3135 for
/// Encarta's maximised 812 × 518 map pane, and it scales with the pane height
/// (`P/m ≈ 6.05 × height`). The styles are compiled for that pane at latitude 0; the client
/// shifts their zoom ranges for its own pane height and latitude (Encarta's scale is the true
/// ground scale at the view centre).
pub const VIEW_P_OVER_M: f64 = 3135.0;
/// `P/m` per pixel of map pane height (VM V1: 3133/518 = 6.05, 2891/477 = 6.06).
pub const VIEW_P_OVER_M_PER_HEIGHT_PX: f64 = 6.05;
/// The pane height the styles are compiled for.
pub const REFERENCE_PANE_HEIGHT_PX: f64 = 518.0;
pub const EARTH_CIRCUMFERENCE_KM: f64 = 40_075.017;

/// Altitude in km at which `zoom` applies (equator).
pub fn altitude_km(zoom: f64) -> f64 {
    EARTH_CIRCUMFERENCE_KM * VIEW_P_OVER_M / (512.0 * 2.5 * 2f64.powf(zoom))
}

/// MapLibre zoom (512-px tiles, equator) of camera altitude `km`.
pub fn zoom_for_altitude(km: f64) -> f64 {
    (EARTH_CIRCUMFERENCE_KM * VIEW_P_OVER_M / (512.0 * 2.5 * km.max(1e-3))).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mercator_round_trip() {
        for lat in [-85.0, -45.0, 0.0, 12.5, 60.0, 85.0] {
            assert!((y_to_lat(lat_to_y(lat)) - lat).abs() < 1e-9);
        }
        assert!((lat_to_y(MAX_LAT)).abs() < 1e-9);
        let (w, s, e, n) = tile_bounds(1, 1, 0);
        assert!((w - 0.0).abs() < 1e-12 && (e - 180.0).abs() < 1e-12 && s.abs() < 1e-9 && (n - MAX_LAT).abs() < 1e-9);
    }

    #[test]
    fn altitude_zoom() {
        // mad-styling.md §2.2 worked example with the VM's P/m = 3135: 150 km → zoom 9.35, 4000 km → 4.62.
        assert!((zoom_for_altitude(150.0) - 9.354).abs() < 0.01);
        assert!((zoom_for_altitude(4000.0) - 4.617).abs() < 0.01);
        assert!((VIEW_P_OVER_M_PER_HEIGHT_PX * REFERENCE_PANE_HEIGHT_PX - VIEW_P_OVER_M).abs() < 0.01 * VIEW_P_OVER_M);
        assert!((altitude_km(zoom_for_altitude(777.0)) - 777.0).abs() < 1e-6);
    }
}
