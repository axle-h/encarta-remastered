/** Pure helpers of the quiz map (no MapLibre import, so tests run in Node). */
import type { StyleSpecification } from "maplibre-gl";
import type { GeoPlace } from "../api";

/** Points the style's `/atlas/…` URLs at `/out/atlas/…`, which both the Rust server and the Vite dev proxy serve. */
export function viaOut(url: string): string {
  return url.replace(/(?<!\/out)\/atlas\//, "/out/atlas/");
}

export function rewriteStyle(style: StyleSpecification): StyleSpecification {
  const s = structuredClone(style);
  if (typeof s.sprite === "string") s.sprite = viaOut(s.sprite);
  if (s.glyphs) s.glyphs = viaOut(s.glyphs);
  for (const src of Object.values(s.sources ?? {})) {
    const x = src as { url?: string; tiles?: string[] };
    if (x.url) x.url = viaOut(x.url);
    if (x.tiles) x.tiles = x.tiles.map(viaOut);
  }
  return s;
}

/** The view for a place: its box (antimeridian-safe) for areas, a regional view around a city. */
export function viewFor(p: GeoPlace): { bounds?: [[number, number], [number, number]]; center?: [number, number]; zoom?: number } {
  if (p.kind === 1 || p.west == null || p.east == null || p.south == null || p.north == null) {
    return { center: [p.lon, p.lat], zoom: 3.6 };
  }
  const east = p.east < p.west ? p.east + 360 : p.east;
  return {
    bounds: [
      [p.west, Math.max(-80, p.south)],
      [east, Math.min(84, p.north)],
    ],
  };
}
