/**
 * Camera altitude ↔ MapLibre zoom, as the converter compiles it into the styles
 * (docs/atlas-output.md "Bands and zooms", mad-styling.md §2.2):
 *
 *   zoom(a) = log2(40075.017 · (P/m) / (512 · 2.5 · a))      a = altitude in km
 *
 * The styles are compiled at latitude 0, so the client uses the same latitude-free mapping for
 * everything (altitude readout, `alt=` in place links, the zoom slider). That keeps the rule that
 * MapLibre applies at a zoom equal to the rule Encarta applies at the displayed altitude. The cost:
 * away from the equator the view is magnified by 1/cos φ against Encarta's (0.5 zoom at 45°).
 */

export const EARTH_CIRCUMFERENCE_KM = 40075.017;
/** Working value of `P/m` until the VM measures it; styles carry their own in `encarta:view_p_over_m`. */
export const DEFAULT_P_OVER_M = 2400;
const TILE = 512;
const FACTOR = 2.5;

export function zoomForAltitude(altitudeKm: number, pOverM = DEFAULT_P_OVER_M): number {
  return Math.log2((EARTH_CIRCUMFERENCE_KM * pOverM) / (TILE * FACTOR * Math.max(altitudeKm, 1e-6)));
}

export function altitudeForZoom(zoom: number, pOverM = DEFAULT_P_OVER_M): number {
  return (EARTH_CIRCUMFERENCE_KM * pOverM) / (TILE * FACTOR * 2 ** zoom);
}

/** Encarta keeps `floor(altitude)` and compares that integer with its rule ranges (mad-styling.md §2.1). */
export function encartaAltitude(zoom: number, pOverM = DEFAULT_P_OVER_M): number {
  return Math.floor(altitudeForZoom(zoom, pOverM));
}

/** Short human altitude: "50,000 km", "3,000 km", "12.5 km". */
export function formatAltitude(km: number): string {
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
