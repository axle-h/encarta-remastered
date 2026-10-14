/**
 * The flat map in Encarta's Miller cylindrical projection (mad-styling.md §12.3; VM V12: the
 * "Flat Map View" is Miller), drawn by MapLibre's Mercator renderer in substitute coordinates.
 *
 * MapLibre has no custom projections, but Miller and Mercator are both cylindrical with x linear
 * in longitude: Mercator y = ln tan(π/4 + φ'/2), Miller y = 1.25 ln tan(π/4 + 0.4 φ). Giving every
 * point the "view latitude" φ' with Mercator y(φ') = Miller y(φ) makes MapLibre's Mercator map a
 * Miller map. The whole sphere (Miller y up to ±2.3034) lies within view latitudes ±78.6°.
 *
 * Tiles are re-made for the view coordinates by two protocols:
 * - `encarta-miller-raster://<RASTER>/<LOOK>/{z}/{x}/{y}`: at small zooms a crop of Encarta's
 *   own flat-map image (`miller/<raster>/<look>-<level>.webp`; in view coordinates it is an affine
 *   image), at larger zooms the rows of the Mercator raster tiles remapped onto view latitudes
 *   (over the Miller image, which fills the polar rows beyond ±85.05°);
 * - `encarta-miller-mvt://<archive URL>/{z}/{x}/{y}`: the vector tiles of the real latitudes the
 *   view tile covers, clipped to their own tile, transformed and clipped again (mvt.ts).
 * The re-making runs in Web Workers (millerTiles.ts, millerProtocol.ts). Everything else (feature ids, properties, the styles) is unchanged; the client converts between
 * real and view latitudes where it talks to the map (camera, search, outline, title).
 */
import { POINT, POLYGON, area2, clipLine, clipRing, type FPt, type Feature, type Layer, type Pt } from "./mvt";

const DEG = Math.PI / 180;
/** Web Mercator latitude limit (the converter's raster and vector tiles stop there). */
export const MERCATOR_MAX_LAT = 85.05112878;

export function millerY(latDeg: number): number {
  const l = Math.max(-90, Math.min(90, latDeg)) * DEG;
  return 1.25 * Math.log(Math.tan(Math.PI / 4 + 0.4 * l));
}

export function millerLat(y: number): number {
  return (2.5 * Math.atan(Math.exp(0.8 * y)) - 0.625 * Math.PI) / DEG;
}

/** Miller y of the poles: 2.3034. */
export const MILLER_Y_MAX = millerY(90);

/** Real latitude → view latitude (the Mercator latitude with the same y). */
export function toViewLat(latDeg: number): number {
  return Math.atan(Math.sinh(millerY(latDeg))) / DEG;
}

/** View latitude → real latitude (clamped to ±90°). */
export function fromViewLat(viewLatDeg: number): number {
  const v = Math.max(-89.9999, Math.min(89.9999, viewLatDeg)) * DEG;
  const y = Math.asinh(Math.tan(v));
  return millerLat(Math.max(-MILLER_Y_MAX, Math.min(MILLER_Y_MAX, y)));
}

/** View latitude of the poles (±78.6°). */
export const VIEW_LAT_MAX = toViewLat(90);

/** Mercator world y in [0, 1] (0 = north) of a latitude, and back. */
export function mercWorldY(latDeg: number): number {
  const l = Math.max(-89.9999, Math.min(89.9999, latDeg)) * DEG;
  return (1 - Math.log(Math.tan(Math.PI / 4 + l / 2)) / Math.PI) / 2;
}
export function mercWorldLat(y: number): number {
  return Math.atan(Math.sinh(Math.PI * (1 - 2 * y))) / DEG;
}

/** World y of a real latitude in view coordinates. */
export const viewWorldY = (realLat: number) => (1 - millerY(realLat) / Math.PI) / 2;
/** Real latitude of a view world y. */
export const realLatOfViewY = (y: number) => millerLat(Math.max(-MILLER_Y_MAX, Math.min(MILLER_Y_MAX, Math.PI * (1 - 2 * y))));

/**
 * The view zoom at which the whole Miller map (pole to pole) is `heightPx` tall: Encarta's flat
 * view opens on the whole world fitted to the pane height (V12: 1.968 px/deg in the 518-px pane).
 */
export function zoomToFitHeight(heightPx: number): number {
  return Math.log2(heightPx / ((MILLER_Y_MAX / Math.PI) * 512));
}

/** Real tile rows (at `zr`) that a view tile row covers: `[first, last]`. */
export function realRows(z: number, y: number, zr: number): [number, number] {
  const n = 2 ** z;
  const top = realLatOfViewY(y / n);
  const bottom = realLatOfViewY((y + 1) / n);
  const m = 2 ** zr;
  const r0 = Math.floor(mercWorldY(Math.min(top, MERCATOR_MAX_LAT)) * m);
  const r1 = Math.floor(mercWorldY(Math.max(bottom, -MERCATOR_MAX_LAT)) * m - 1e-9);
  return [Math.max(0, Math.min(m - 1, r0)), Math.max(0, Math.min(m - 1, r1))];
}

/** The real zoom to read for view tile `(z, y)`: `z`, or coarser while that needs more than `maxRows` rows. */
export function realZoom(z: number, y: number, maxZoom: number, maxRows = 4): number {
  let zr = Math.min(z, maxZoom);
  while (zr > 0) {
    const [a, b] = realRows(z, y, zr);
    if (b - a + 1 <= maxRows) break;
    zr--;
  }
  return zr;
}

// ---------------------------------------------------------------------------------------------
// Vector tiles

const BUFFER = 64;

/** Re-tiles real MVT tiles (with their `(zr, x, y)`) into view tile `(z, x, y)`. Pure, for tests. */
export function retile(sources: readonly { zr: number; x: number; y: number; layers: Layer[] }[], z: number, x: number, y: number, extent = 4096): Layer[] {
  const out = new Map<string, Layer>();
  const n = 2 ** z;
  for (const src of sources) {
    const m = 2 ** src.zr;
    for (const layer of src.layers) {
      const e = layer.extent;
      const toView = (p: Pt): FPt => {
        const wx = (src.x + p[0] / e) / m;
        const lat = mercWorldLat((src.y + p[1] / e) / m);
        const wy = viewWorldY(lat);
        return [(wx * n - x) * extent, (wy * n - y) * extent];
      };
      let target = out.get(layer.name);
      if (!target) out.set(layer.name, (target = { name: layer.name, extent, features: [] }));
      for (const f of layer.features) {
        const parts: Pt[][] = [];
        if (f.type === POINT) {
          const pts = f.parts
            .flat()
            .filter((p) => p[0] >= 0 && p[0] < e && p[1] >= 0 && p[1] < e)
            .map(toView)
            .filter((p) => p[0] >= 0 && p[0] < extent && p[1] >= 0 && p[1] < extent)
            .map((p): Pt => [Math.round(p[0]), Math.round(p[1])]);
          if (pts.length) parts.push(pts);
        } else if (f.type === POLYGON) {
          let keepHoles = false;
          for (const ring of f.parts) {
            const exterior = area2(ring) > 0;
            if (!exterior && !keepHoles) continue;
            const own = clipRing(ring as FPt[], 0, 0, e, e);
            if (own.length < 3) {
              if (exterior) keepHoles = false;
              continue;
            }
            const v = clipRing(own.map((p) => toView(p as Pt)), -BUFFER, -BUFFER, extent + BUFFER, extent + BUFFER);
            const q = dedupe(v);
            if (q.length < 3 || area2(q) === 0) {
              if (exterior) keepHoles = false;
              continue;
            }
            if (exterior) keepHoles = true;
            parts.push(q);
          }
        } else {
          for (const line of f.parts) {
            for (const own of clipLine(line as FPt[], 0, 0, e, e)) {
              for (const v of clipLine(own.map((p) => toView(p as Pt)), -BUFFER, -BUFFER, extent + BUFFER, extent + BUFFER)) {
                const q = dedupe(v);
                if (q.length >= 2) parts.push(q);
              }
            }
          }
        }
        if (parts.length) target.features.push({ id: f.id, type: f.type, props: f.props, parts } as Feature);
      }
    }
  }
  return [...out.values()];
}

function dedupe(pts: readonly FPt[]): Pt[] {
  const out: Pt[] = [];
  for (const p of pts) {
    const q: Pt = [Math.round(p[0]), Math.round(p[1])];
    const last = out[out.length - 1];
    if (!last || last[0] !== q[0] || last[1] !== q[1]) out.push(q);
  }
  if (out.length > 1) {
    const a = out[0]!;
    const b = out[out.length - 1]!;
    if (a[0] === b[0] && a[1] === b[1]) out.pop();
  }
  return out;
}

export const MILLER_MVT_PROTOCOL = "encarta-miller-mvt";
export const MILLER_RASTER_PROTOCOL = "encarta-miller-raster";

/** Source URL for a vector archive in the flat view. `maxzoom` = the archive's own. */
export function millerVectorTiles(archiveUrl: string, maxzoom: number): string {
  return `${MILLER_MVT_PROTOCOL}://${maxzoom}/${encodeURIComponent(archiveUrl)}/{z}/{x}/{y}`;
}

export function parseVectorUrl(url: string): { maxzoom: number; archive: string; z: number; x: number; y: number } | null {
  const m = new RegExp(`^${MILLER_MVT_PROTOCOL}://(\\d+)/([^/]+)/(\\d+)/(\\d+)/(\\d+)`).exec(url);
  if (!m) return null;
  return { maxzoom: Number(m[1]), archive: decodeURIComponent(m[2]!), z: Number(m[3]), x: Number(m[4]), y: Number(m[5]) };
}

// ---------------------------------------------------------------------------------------------
// Raster tiles

export interface MillerImage {
  raster: string;
  level: number;
  width: number;
  height: number;
}

/** `encarta-miller-raster://` URL of a style's raster in the flat view. */
export function millerRasterTiles(raster: string, look: string, archiveUrl: string): string {
  return `${MILLER_RASTER_PROTOCOL}://${raster.toUpperCase()}/${look.toUpperCase()}/${encodeURIComponent(archiveUrl)}/{z}/{x}/{y}`;
}

export function parseRasterUrl(url: string): { raster: string; look: string; archive: string; z: number; x: number; y: number } | null {
  const m = new RegExp(`^${MILLER_RASTER_PROTOCOL}://([^/]+)/([^/]+)/([^/]+)/(\\d+)/(\\d+)/(\\d+)`).exec(url);
  if (!m) return null;
  return { raster: m[1]!, look: m[2]!, archive: decodeURIComponent(m[3]!), z: Number(m[4]), x: Number(m[5]), y: Number(m[6]) };
}

/** Source rectangle of view tile `(z, x, y)` in a Miller image `w × h` (it is affine in view coordinates). */
export function millerImageRect(w: number, h: number, z: number, x: number, y: number): { sx: number; sy: number; size: number } {
  const size = w / 2 ** z;
  return { sx: x * size, sy: y * size + (h - w) / 2, size };
}

/** The Miller level to crop for zoom `z`: the smallest whose tiles are at least 256 px, else the largest. */
export function millerLevel(images: readonly MillerImage[], z: number): MillerImage | undefined {
  const sorted = [...images].sort((a, b) => a.level - b.level);
  return sorted.find((m) => m.width / 2 ** z >= 256) ?? sorted[sorted.length - 1];
}

export const TILE = 256;
/** Below this zoom the flat view shows Encarta's own flat-map images; above, the remapped Mercator raster. */
export const MILLER_IMAGE_MAX_ZOOM = 4;
