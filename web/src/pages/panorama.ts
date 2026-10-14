/**
 * Encarta's panoramic ("2D") virtual tours, as the DATA records describe them (content-schema §3;
 * read from the data, not yet compared with the VM):
 * - the tour (`virttour`, `type="2DVT"` or an untyped `vtour` icon, 16 of them: Westminster Abbey, Alcatraz …) lists its
 *   scenes as `children/media` and an introduction as `children/article`;
 * - a scene has one wide `surround` picture (`.svj`, JPEG, 1440–3120 × 308–500, the panorama's
 *   `width`), `label`s (text at `x`,`y` in panorama pixels), `hotspot`s (`shed` = a rectangle
 *   in panorama pixels, `destmedia` = the scene or picture it opens), the pictures those hotspots
 *   open (`media`), an `article` and a `site` (the site map: a `vtour` record with a map picture and
 *   hotspots of its own that lead to scenes).
 */
import type { Item } from "../api/types";

/** `type="2DVT"`; two tours (Cheese Factory, Theater Backstage) have an empty type and only the `vtour` icon. */
export function isPanoramicTour(item: Pick<Item, "meta"> | undefined): boolean {
  const a = item?.meta?.source_attrs as { type?: string; ticon?: string; group?: string } | undefined;
  if (a?.type === "2DVT") return true;
  return a?.group === "virttour" && a.ticon === "vtour" && !a.type;
}

export function childIds(item: Pick<Item, "children"> | undefined, kind: string): number[] {
  return (item?.children ?? []).filter((c) => c.kind === kind).map((c) => c.ref_id);
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A hotspot's `shed` rectangle (two corners, either order); null when malformed. */
export function shedRect(meta: Record<string, unknown> | undefined): Rect | null {
  const shed = meta?.shed;
  if (!Array.isArray(shed) || shed.length < 2) return null;
  const pts = shed.slice(0, 2).map((p) => (Array.isArray(p) ? p.map(Number) : []));
  if (pts.some((p) => p.length < 2 || p.some((v) => !Number.isFinite(v)))) return null;
  const [[x1, y1], [x2, y2]] = pts as [number[], number[]];
  return { x: Math.min(x1!, x2!), y: Math.min(y1!, y2!), w: Math.max(4, Math.abs(x2! - x1!)), h: Math.max(4, Math.abs(y2! - y1!)) };
}

/** A label's position (`x`, `y` attributes), or null. */
export function labelPoint(meta: Record<string, unknown> | undefined): { x: number; y: number } | null {
  const a = meta?.source_attrs as { x?: string; y?: string } | undefined;
  const x = Number(a?.x);
  const y = Number(a?.y);
  return a?.x !== undefined && a?.y !== undefined && Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null;
}

export function destOf(item: Pick<Item, "meta"> | undefined): number | null {
  const d = Number((item?.meta?.source_attrs as { destmedia?: string } | undefined)?.destmedia);
  return Number.isFinite(d) && d > 0 ? d : null;
}

/**
 * A hotspot's `quadrant` (four points; 287 of 318 make a 10 × 24 box beside or inside its `shed`,
 * 19 are 0 wide where the box was cut at the picture's right edge) as a marker rectangle, or null
 * when malformed. Width is kept at least 4 and boxes wider than 60 (one record: 120,010) are dropped.
 */
export function quadrantRect(meta: Record<string, unknown> | undefined): Rect | null {
  const q = meta?.quadrant;
  if (!Array.isArray(q) || q.length < 2) return null;
  const pts = q.map((p) => (Array.isArray(p) ? p.map(Number) : []));
  if (pts.some((p) => p.length < 2 || p.some((v) => !Number.isFinite(v)))) return null;
  const xs = pts.map((p) => p[0]!);
  const ys = pts.map((p) => p[1]!);
  const x0 = Math.min(...xs);
  const y0 = Math.min(...ys);
  const w = Math.max(...xs) - x0;
  const h = Math.max(...ys) - y0;
  if (w > 60 || h > 80 || h <= 0) return null;
  return w >= 4 ? { x: x0, y: y0, w, h } : { x: x0 - 4, y: y0, w: 4, h };
}
