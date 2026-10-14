/**
 * Hit-testing: what is under the cursor, in the order Encarta's pop-up menu lists it (V11 shot:
 * "Limoges, Auvergne, Western Europe, West Central Europe, Massif central, Loire Basin, France"):
 * points first, then lines, then areas, the country last. Every feature's MVT id is its entity id.
 */
import type { StyleSpecification } from "maplibre-gl";
import { inZoom, isHoverLayer, layerClasses } from "./layers";

export interface QueriedFeature {
  id?: string | number;
  sourceLayer?: string;
  source?: string;
  layer?: { id: string; type: string };
  properties?: Record<string, unknown> | null;
}

export interface HitPlace {
  entity: number;
  name: string;
  className: string;
  classId: number | null;
  kind: "point" | "line" | "area";
  /** Sprite name of the symbol the feature is drawn with, when it is a point. */
  icon?: string;
}

const COUNTRY = /^(Sovereign|CountryRegion)/;
const WORLD = /^(World|Ocean|Continent)$/;

function kindOf(f: QueriedFeature): HitPlace["kind"] {
  if (f.sourceLayer === "points") return "point";
  if (f.sourceLayer === "lines") return "line";
  return "area"; // areas and their label anchors (`labels`)
}

function areaRank(className: string): number {
  if (WORLD.test(className)) return 2;
  if (COUNTRY.test(className)) return 1;
  return 0;
}

/**
 * The hit list: named features under the cursor whose class is in the style's hit list at this
 * zoom (`allowed`, from layers.ts `hitRanges`: PDCAltitudeGrid's interactive flag), once per entity.
 */
export function rankHits(
  features: readonly QueriedFeature[],
  iconOf?: (f: QueriedFeature) => string | undefined,
  allowed?: ReadonlySet<number>,
): HitPlace[] {
  const seen = new Map<number, HitPlace>();
  const order: HitPlace[] = [];
  for (const f of features) {
    const p = f.properties ?? {};
    const entity = typeof f.id === "number" ? f.id : Number(f.id);
    const name = typeof p.name === "string" ? p.name : "";
    if (!Number.isFinite(entity) || !name || p.anonymous === true || p.anonymous === 1) continue;
    if (allowed && !(typeof p.class === "number" && allowed.has(p.class))) continue;
    const kind = kindOf(f);
    // Water parts of split areas: Encarta lists no land area over the sea (Brittany coast, VM).
    if (kind === "area" && p.water === true) continue;
    const prev = seen.get(entity);
    if (prev) {
      // A label anchor and its area are one place; a point beats both.
      if (kind === "point" && prev.kind !== "point") prev.kind = "point";
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
    seen.set(entity, hit);
    order.push(hit);
  }
  const kindRank = { point: 0, line: 1, area: 2 } as const;
  return order
    .map((h, i) => ({ h, i }))
    .sort((a, b) => kindRank[a.h.kind] - kindRank[b.h.kind] || (a.h.kind === "area" ? areaRank(a.h.className) - areaRank(b.h.className) : 0) || a.i - b.i)
    .map(({ h }) => h);
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
