/**
 * Hit-testing: what is under the cursor, in the order of Encarta's pop-up menu (mad-styling.md §4.5,
 * from MOBB's hit-list sort and the VM menus): places hit through their drawn label first, then
 * points, routable roads, other lines and areas; within each, the class's PDCAttribute scale
 * ascending, then GeoObj (key, entry) descending. V11: "Limoges, Auvergne, Western Europe, West
 * Central Europe, Massif central, Loire Basin, France". Every feature's MVT id is its entity id.
 */
import type { StyleSpecification } from "maplibre-gl";
import { HIT_SIDE_LAND, HIT_SIDE_WATER, inZoom, isHoverLayer, layerClasses } from "./layers";

export interface QueriedFeature {
  id?: string | number;
  sourceLayer?: string;
  source?: string;
  layer?: { id: string; type: string };
  properties?: Record<string, unknown> | null;
  geometry?: { type: string; coordinates?: unknown };
}

export interface HitPlace {
  entity: number;
  name: string;
  className: string;
  classId: number | null;
  kind: "point" | "line" | "area";
  /** Sprite name of the symbol the feature is drawn with, when it is a point. */
  icon?: string;
  /** Hit through its drawn label (text or shield), not only its symbol or geometry. */
  label?: boolean;
}

/** What the menu sorts by besides the kind of hit (style metadata, atlas-output.md). */
export interface HitOrder {
  /** PDCAttribute scale / 1000 by class row (`encarta:class_scale`). */
  scale: ReadonlyMap<number, number>;
  /** Rows with the routable-road flag, PDCAttribute byte 6 (`encarta:routable_classes`). */
  routable: ReadonlySet<number>;
}

export interface RankOptions {
  order?: HitOrder;
  /**
   * Whether a feature was hit through its label. By default a symbol on `labels` (area names) or
   * `lines` (line names, shields) is a label hit and a point is not; points draw symbol and name as
   * one MapLibre symbol, so the caller tells them apart by where the click is.
   */
  labelHit?: (f: QueriedFeature) => boolean;
}

/** MOBB keeps at most four routable-road hits (mad-styling.md §4.5). */
const MAX_ROUTABLE_HITS = 4;

const COUNTRY = /^(Sovereign|CountryRegion)/;
const WORLD = /^(World|Ocean|Continent)$/;

function kindOf(f: QueriedFeature): HitPlace["kind"] {
  if (f.sourceLayer === "points") return "point";
  if (f.sourceLayer === "lines") return "line";
  return "area"; // areas and their label anchors (`labels`)
}

function defaultLabelHit(f: QueriedFeature): boolean {
  return f.layer?.type === "symbol" && (f.sourceLayer === "labels" || f.sourceLayer === "lines");
}

/** Without class scales (older styles): countries, then World/Ocean/Continent, after other areas. */
function areaRank(className: string): number {
  if (WORLD.test(className)) return 2;
  if (COUNTRY.test(className)) return 1;
  return 0;
}

/** MOBB's rank of a hit: label of a routable road 0, other labels 1, points 2, routable roads 3, lines 4, areas 5. */
export function hitRank(h: Pick<HitPlace, "kind" | "label" | "classId">, routable?: ReadonlySet<number>): number {
  const road = h.classId !== null && routable?.has(h.classId) === true;
  if (h.label) return road ? 0 : 1;
  if (road) return 3;
  return { point: 2, line: 4, area: 5 }[h.kind];
}

/**
 * The hit list: named features under the cursor whose class is in the style's hit list at this
 * zoom (`allowed`, from layers.ts `hitClassesAt`: PDCAltitudeGrid's interactive flag and, as a map,
 * each class's hit side there), once per entity, in Encarta's menu order (`hitRank`, then class
 * scale, then GeoObj descending; the `geoobj` tile property). Hit sides (mad-styling.md §4.4, VM): a
 * water class (Bay of Biscay) is listed only over water, i.e. on the water part of its split
 * polygon; when one is listed, land classes (France, Brittany, Europe) are left out. Elsewhere land
 * classes are listed over the sea too (Jersey a few km offshore), and side-0 classes (parks,
 * regions, World) anywhere.
 */
export function rankHits(
  features: readonly QueriedFeature[],
  iconOf?: (f: QueriedFeature) => string | undefined,
  allowed?: ReadonlySet<number> | ReadonlyMap<number, number>,
  options: RankOptions = {},
): HitPlace[] {
  const { order, labelHit = defaultLabelHit } = options;
  const seen = new Map<number, HitPlace>();
  const hits: HitPlace[] = [];
  const side = new Map<HitPlace, number>();
  const objOrder = new Map<HitPlace, number>();
  let overWater = false;
  for (const f of features) {
    const p = f.properties ?? {};
    const entity = typeof f.id === "number" ? f.id : Number(f.id);
    const name = typeof p.name === "string" ? p.name : "";
    if (!Number.isFinite(entity) || !name || p.anonymous === true || p.anonymous === 1) continue;
    if (allowed && !(typeof p.class === "number" && allowed.has(p.class))) continue;
    const kind = kindOf(f);
    const s = allowed instanceof Map && typeof p.class === "number" ? (allowed.get(p.class) ?? 0) : 0;
    if (s === HIT_SIDE_WATER && f.sourceLayer === "areas") {
      if (p.water !== true) continue;
      overWater = true;
    }
    const label = labelHit(f);
    const prev = seen.get(entity);
    if (prev) {
      // One entry per place (MOBB merges by GeoObj): its label and symbol hits add up, a point beats
      // its area or label anchor.
      if (kind === "point" && prev.kind !== "point") prev.kind = "point";
      if (label) prev.label = true;
      if (!prev.icon) prev.icon = iconOf?.(f);
      continue;
    }
    const hit: HitPlace = {
      entity,
      name,
      className: typeof p.class_name === "string" ? p.class_name : "",
      classId: typeof p.class === "number" ? p.class : null,
      kind,
      icon: iconOf?.(f),
    };
    if (label) hit.label = true;
    seen.set(entity, hit);
    side.set(hit, s);
    if (typeof p.geoobj === "number") objOrder.set(hit, p.geoobj);
    hits.push(hit);
  }
  const scaleOf = (h: HitPlace) => (h.classId === null ? undefined : order?.scale.get(h.classId));
  const sorted = hits
    .filter((h) => !(overWater && side.get(h) === HIT_SIDE_LAND))
    .map((h, i) => ({ h, i, rank: hitRank(h, order?.routable), scale: scaleOf(h), obj: objOrder.get(h) }))
    .sort((a, b) => {
      if (a.rank !== b.rank) return a.rank - b.rank;
      if (a.scale !== undefined && b.scale !== undefined) {
        if (a.scale !== b.scale) return a.scale - b.scale;
      } else if (a.h.kind === "area" && b.h.kind === "area") {
        const d = areaRank(a.h.className) - areaRank(b.h.className);
        if (d) return d;
      }
      if (a.obj !== undefined && b.obj !== undefined && a.obj !== b.obj) return b.obj - a.obj;
      return a.i - b.i;
    })
    .map(({ h }) => h);
  let roads = 0;
  return sorted.filter((h) => !(h.classId !== null && order?.routable.has(h.classId) && ++roads > MAX_ROUTABLE_HITS));
}

/**
 * Label-hit test for a click at `at` (screen px): a point symbol counts as a label hit when the click
 * is off its icon (`iconRadius` px around the projected point, plus `pad`) or when it has no icon (a
 * name without a symbol); other features as in `defaultLabelHit`.
 */
export function labelHitAt(
  at: { x: number; y: number },
  project: (lngLat: [number, number]) => { x: number; y: number },
  iconRadius: (f: QueriedFeature) => number | undefined,
  pad = 2,
): (f: QueriedFeature) => boolean {
  return (f) => {
    if (f.sourceLayer !== "points") return defaultLabelHit(f);
    if (f.layer?.type !== "symbol") return false;
    const r = iconRadius(f);
    if (r === undefined) return true;
    const c = f.geometry?.type === "Point" ? (f.geometry.coordinates as [number, number] | undefined) : undefined;
    if (!c) return false;
    const p = project(c);
    return Math.hypot(p.x - at.x, p.y - at.y) > r + pad;
  };
}

/** Class rows that have a hover highlight layer (b15/b16/w17) at this zoom. */
export function hoverClassesAt(style: StyleSpecification, zoom: number): Set<number> {
  const out = new Set<number>();
  for (const layer of style.layers) {
    if (!isHoverLayer(layer) || !inZoom(layer, zoom)) continue;
    for (const id of layerClasses(layer).ids) out.add(id);
  }
  return out;
}

const WATER = /^(Lake|Inland sea|Reservoir|Lake\/river)/;

function hoverRank(h: HitPlace): number {
  if (h.kind !== "area") return 0;
  if (WATER.test(h.className)) return 1;
  if (COUNTRY.test(h.className)) return 2;
  return 3;
}

/**
 * The place highlighted under the cursor: a hit whose class has a highlight pen. Encarta highlights
 * the country under the mouse (V11: France in Political), a lake over its country (Lake Geneva), so
 * lines and water come first, then countries, then other areas (basins, regions).
 */
export function hoverTarget(hits: readonly HitPlace[], hoverable: ReadonlySet<number>): HitPlace | undefined {
  let best: HitPlace | undefined;
  for (const h of hits) {
    if (h.classId === null || !hoverable.has(h.classId)) continue;
    if (!best || hoverRank(h) < hoverRank(best)) best = h;
  }
  return best;
}

/** Title over the map ("Germany | Europe", or "World" from far out): the country under the centre and its continent. */
export function viewTitle(hits: readonly HitPlace[], altitudeKm: number, body: "earth" | "moon"): { title: string; subtitle?: string; entity?: number } {
  if (body === "moon") return { title: "Moon" };
  if (altitudeKm >= 14000) return { title: "World" };
  const country = hits.find((h) => /^Sovereign/.test(h.className)) ?? hits.find((h) => /^CountryRegion/.test(h.className));
  const continent = hits.find((h) => h.className === "Continent");
  const water = hits.find((h) => /^(Ocean|Water label - major|Sea|Inland sea)$/.test(h.className));
  // Below 2,000 km (band 6) Encarta names the first-level division and its country (V9: "Wien | Austria").
  const admin = altitudeKm < 2000 ? hits.find((h) => /^Admin ?1/.test(h.className)) : undefined;
  if (admin && country) return { title: admin.name, subtitle: country.name, entity: admin.entity };
  const main = country ?? water ?? continent;
  if (!main) return { title: "World" };
  const subtitle = main === continent ? undefined : continent?.name;
  return { title: main.name, subtitle, entity: main.entity };
}
