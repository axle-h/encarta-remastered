/**
 * Camera altitude ↔ MapLibre zoom.
 *
 * Encarta draws `P / (2.5·m·a)` pixels per km at the view centre (mad-styling.md §2.1), the true
 * ground scale, isotropic, at any latitude (VM V1/V2, docs/vm-findings.md). `P/m` grows with the
 * map pane: 3135 for its maximised 812 × 518 pane, `P/m ≈ 6.05 × pane height`. MapLibre (globe and
 * Mercator alike) draws `512·2^z / (40075.017·cos φ)` pixels per km at latitude φ, so
 *
 *   zoom(a, φ) = log2(40075.017 · cos φ · (P/m) / (512 · 2.5 · a))      a = altitude in km
 *
 * The converter compiles the styles' zoom ranges at φ = 0 for the reference pane
 * (`encarta:view_p_over_m`, docs/atlas-output.md). The client shifts every layer by
 * `zoom(a, φ, P/m of its own pane) − zoom(a, 0, reference P/m)` ([`zoomShift`]), so that MapLibre
 * applies, at the displayed ground scale, the rule Encarta applies at that altitude.
 */

export const EARTH_CIRCUMFERENCE_KM = 40075.017;
/** `P/m` of Encarta's maximised map pane (812 × 518), measured in the VM; the styles carry it as `encarta:view_p_over_m`. */
export const DEFAULT_P_OVER_M = 3135;
/** `P/m` per pixel of pane height (VM: 3133 / 518 and 2891 / 477). */
export const P_OVER_M_PER_HEIGHT_PX = 6.05;
const TILE = 512;
const FACTOR = 2.5;
/** cos φ is not allowed below this near the poles. */
const MIN_COS = 0.05;

/** `P/m` for a map pane `heightPx` tall. */
export function pOverMForPane(heightPx: number, perPx = P_OVER_M_PER_HEIGHT_PX): number {
  return heightPx > 0 ? perPx * heightPx : DEFAULT_P_OVER_M;
}

/** Zoom of altitude `altitudeKm` at the equator (the converter's mapping). */
export function zoomForAltitude(altitudeKm: number, pOverM = DEFAULT_P_OVER_M): number {
  return Math.log2((EARTH_CIRCUMFERENCE_KM * pOverM) / (TILE * FACTOR * Math.max(altitudeKm, 1e-6)));
}

export function altitudeForZoom(zoom: number, pOverM = DEFAULT_P_OVER_M): number {
  return (EARTH_CIRCUMFERENCE_KM * pOverM) / (TILE * FACTOR * 2 ** zoom);
}

/**
 * The flat (Miller) view draws `P / (5·m·a)` pixels per km along the equator, half the globe's
 * (mad-styling.md §2.1, from the code; the VM did not measure it), so the same altitude is one zoom
 * level further out. The Miller map's scale is fixed per zoom, so the flat view has no cos φ term.
 */
export const FLAT_ZOOM_OFFSET = -1;

const latTerm = (flat: boolean, lat: number) => (flat ? FLAT_ZOOM_OFFSET : Math.log2(Math.max(Math.cos((lat * Math.PI) / 180), MIN_COS)));

/** MapLibre zoom showing `altitudeKm` at latitude `lat` (globe: true ground scale). */
export function zoomForView(altitudeKm: number, flat: boolean, pOverM = DEFAULT_P_OVER_M, lat = 0): number {
  return zoomForAltitude(altitudeKm, pOverM) + latTerm(flat, lat);
}

export function altitudeForView(zoom: number, flat: boolean, pOverM = DEFAULT_P_OVER_M, lat = 0): number {
  return altitudeForZoom(zoom - latTerm(flat, lat), pOverM);
}

/**
 * How far the style's zoom ranges (compiled at φ = 0 with `stylePOverM`) move for this view:
 * add it to every layer's minzoom/maxzoom.
 */
export function zoomShift(flat: boolean, pOverM: number, stylePOverM: number, lat = 0): number {
  return Math.log2(pOverM / stylePOverM) + latTerm(flat, lat);
}

/** A shift rounded to 0.05 zoom, so small moves do not restyle the map. */
export function quantiseShift(s: number): number {
  return Math.round(s * 20) / 20;
}

/** Encarta keeps `floor(altitude)` and compares that integer with its rule ranges (mad-styling.md §2.1). */
export function encartaAltitude(zoom: number, pOverM = DEFAULT_P_OVER_M): number {
  return Math.floor(altitudeForZoom(zoom, pOverM));
}

/** Short human altitude: "50,000 km", "3,000 km", "12.5 km". */
export function formatAltitude(km: number, units: "metric" | "us" = "metric"): string {
  if (units === "us") {
    const mi = km / 1.609344;
    return mi >= 100 ? `${Math.round(mi).toLocaleString("en-US")} mi` : `${mi.toFixed(mi >= 10 ? 0 : 1)} mi`;
  }
  if (km >= 100) return `${Math.round(km).toLocaleString("en-US")} km`;
  if (km >= 10) return `${km.toFixed(0)} km`;
  return `${km.toFixed(1)} km`;
}

/** A style's altitude limits (metadata `encarta:minzoom/maxzoom`), widened by any MinRect the point lies in. */
export interface ZoomLimits {
  minzoom: number;
  maxzoom: number;
}

export interface MinRect {
  /** [west, south, east, north] in degrees. */
  bbox: [number, number, number, number];
  maxzoom: number;
  min_altitude_km?: number;
}

export function insideBbox(lon: number, lat: number, b: readonly [number, number, number, number]): boolean {
  const [w, s, e, n] = b;
  if (lat < s || lat > n) return false;
  const x = ((((lon - w) % 360) + 360) % 360) + w; // normalise lon into [w, w + 360)
  return x <= (e >= w ? e : e + 360);
}

/**
 * The maximum zoom at a point: the style's own, or a MinRect's (mad-styling.md §13.2: inside these
 * boxes a style may zoom down to 5 km) when the point is inside one.
 */
export function maxZoomAt(lon: number, lat: number, base: number, rects: readonly MinRect[] | undefined): number {
  let z = base;
  for (const r of rects ?? []) if (r.maxzoom > z && insideBbox(lon, lat, r.bbox)) z = r.maxzoom;
  return z;
}

/** The view a `msencarta://view=MapView/cmd=map/…` URL asks for (mad-styling.md §2.3). */
export interface MsEncartaView {
  lat?: number;
  lon?: number;
  altitudeKm?: number;
  entity?: number;
  /** Encarta's style-menu number (not a MapStyle id; see catalog.ts `MENU_STYLE`). */
  menuStyle?: number;
}

export function parseMsEncartaUrl(url: string): MsEncartaView | null {
  const m = /^msencarta:\/\/(.*)$/i.exec(url.trim());
  if (!m || !m[1]) return null;
  const out: MsEncartaView = {};
  for (const part of m[1].split("/")) {
    const eq = part.indexOf("=");
    if (eq < 0) continue;
    const k = part.slice(0, eq).toLowerCase();
    const v = Number(part.slice(eq + 1));
    if (!Number.isFinite(v)) continue;
    if (k === "lat") out.lat = v;
    else if (k === "long" || k === "lon") out.lon = v;
    else if (k === "alt") out.altitudeKm = v;
    else if (k === "eid") out.entity = v;
    else if (k === "style") out.menuStyle = v;
  }
  return out;
}
