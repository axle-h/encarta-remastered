/**
 * Reading the converted styles: which PDCAttribute classes a layer draws, which layers are hover
 * highlights, which are optional ("show more"), and the extra layers this client adds (hit-testing,
 * statistics). Layer conventions: docs/atlas-output.md "Styles".
 */
import type { FilterSpecification, LayerSpecification, StyleSpecification } from "maplibre-gl";

export const HIT_PREFIX = "encarta-hit-";
export const STAT_LAYER = "encarta-stat-fill";
export const LOOK_SOURCE = "encarta-look";

export interface LayerClasses {
  ids: number[];
  names: string[];
}

type AnyLayer = LayerSpecification & {
  metadata?: Record<string, unknown>;
  filter?: unknown;
  layout?: Record<string, unknown>;
  paint?: Record<string, unknown>;
  source?: string;
  "source-layer"?: string;
  minzoom?: number;
  maxzoom?: number;
};

/** Class rows of a layer's filter (`["==", ["get","class"], n]` or `["in", ["get","class"], ["literal", […]]]`), paired with its metadata names. */
export function layerClasses(layer: LayerSpecification): LayerClasses {
  const l = layer as AnyLayer;
  const names = Array.isArray(l.metadata?.["encarta:classes"]) ? (l.metadata!["encarta:classes"] as string[]) : [];
  const ids: number[] = [];
  const walk = (f: unknown): void => {
    if (!Array.isArray(f)) return;
    const [op, a, b] = f as unknown[];
    const isClassGet = Array.isArray(a) && a[0] === "get" && a[1] === "class";
    if (op === "==" && isClassGet && typeof b === "number") ids.push(b);
    else if (op === "in" && isClassGet && Array.isArray(b) && b[0] === "literal" && Array.isArray(b[1])) ids.push(...(b[1] as number[]));
    else for (const x of f) walk(x);
  };
  walk(l.filter);
  return { ids, names };
}

/** `class row → class name` over a whole style. */
export function classNames(style: StyleSpecification): Map<number, string> {
  const out = new Map<number, string>();
  for (const layer of style.layers) {
    const { ids, names } = layerClasses(layer);
    if (ids.length === names.length) ids.forEach((id, i) => out.set(id, names[i]!));
  }
  return out;
}

export function isHoverLayer(layer: LayerSpecification): boolean {
  const paint = (layer as AnyLayer).paint;
  return !!paint && JSON.stringify(paint).includes('"feature-state","hover"');
}

export function isOptional(layer: LayerSpecification): boolean {
  return (layer as AnyLayer).layout?.visibility === "none";
}

export function inZoom(layer: LayerSpecification, z: number): boolean {
  const l = layer as AnyLayer;
  return z >= (l.minzoom ?? 0) && z < (l.maxzoom ?? 24);
}

/** Vector sources of a style (earth, earth-detail, moon). */
export function vectorSources(style: StyleSpecification): string[] {
  return Object.entries(style.sources)
    .filter(([, s]) => s.type === "vector")
    .map(([id]) => id);
}

/** `[minzoom, maxzoom)` ranges. */
export type Ranges = [number, number][];

/**
 * The style's per-class interactive ranges (metadata `encarta:interactive`, from PDCAltitudeGrid
 * `b4`, mad-styling.md §4.1): `class → [[minzoom, maxzoom, tri-state], …]`, tri-state 1 = by
 * default, 2 = when the user turns the class on.
 */
export function interactiveRanges(style: StyleSpecification): Map<number, [number, number, number][]> {
  const m = (style.metadata ?? {}) as Record<string, unknown>;
  const raw = m["encarta:interactive"];
  const out = new Map<number, [number, number, number][]>();
  if (raw && typeof raw === "object") {
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      if (Array.isArray(v)) out.set(Number(k), v.filter((r): r is [number, number, number] => Array.isArray(r) && r.length === 3));
    }
  }
  return out;
}

/** Classes the click menu never lists (metadata `encarta:menu_excluded_classes`: PDCAttribute pen group 134). */
export function menuExcludedClasses(style: StyleSpecification): Set<number> {
  const raw = ((style.metadata ?? {}) as Record<string, unknown>)["encarta:menu_excluded_classes"];
  return new Set(Array.isArray(raw) ? raw.filter((c): c is number => typeof c === "number") : []);
}

function union(r: Ranges): Ranges {
  const s = [...r].sort((a, b) => a[0] - b[0]);
  const out: Ranges = [];
  for (const [a, b] of s) {
    const last = out[out.length - 1];
    if (last && a <= last[1] + 0.02) last[1] = Math.max(last[1], b);
    else out.push([a, b]);
  }
  return out;
}

/**
 * Where each class goes into the hit list: where its rules make it interactive (`b4`), with the
 * user's class overrides (off = never, on = also where the rule makes it optional). Drawing is not
 * required: the VM's click menu (V11) lists statistical regions and basins that Political neither
 * draws nor labels. It leaves out the classes of PDCAttribute pen group 134 (time zones, DST areas,
 * "Shading - gray"), as ENCARTA.EXE does (metadata `encarta:menu_excluded_classes`, mad-styling.md §16 V11).
 */
export function hitRanges(style: StyleSpecification, o: ClassOverrides): Map<number, Ranges> {
  const out = new Map<number, Ranges>();
  const excluded = menuExcludedClasses(style);
  for (const [cls, rules] of interactiveRanges(style)) {
    if (o.off.has(cls) || excluded.has(cls)) continue;
    const r = union(rules.filter((x) => x[2] === 1 || (x[2] === 2 && o.on.has(cls))).map((x): [number, number] => [x[0], x[1]]));
    if (r.length) out.set(cls, r);
  }
  return out;
}

/** Classes in the hit list at `zoom`. */
export function hitClassesAt(ranges: ReadonlyMap<number, Ranges>, zoom: number): Set<number> {
  const out = new Set<number>();
  for (const [cls, r] of ranges) if (r.some(([a, b]) => zoom >= a && zoom < b)) out.add(cls);
  return out;
}

const NAMED: FilterSpecification = ["all", ["has", "name"], ["!=", ["get", "name"], ""], ["!", ["to-boolean", ["coalesce", ["get", "anonymous"], false]]]];

/** Not the water part of a split polygon: the click menu lists no land area over the sea (VM, Brittany coast). */
const LAND_PART: FilterSpecification = ["!", ["to-boolean", ["coalesce", ["get", "water"], false]]];

/**
 * Invisible layers for hit-testing the `areas` layer, so a click can list everything under the
 * cursor as Encarta's pop-up menu does (V11), including label-only areas that no layer draws
 * (regions, basins). One layer per zoom range with the classes in the hit list there; lines and
 * points are hit-tested on their drawn layers. MapLibre queries layers by geometry, not opacity.
 * Water parts of split polygons (`water: true`) are left out.
 */
export function hitLayers(style: StyleSpecification, ranges: ReadonlyMap<number, Ranges>): LayerSpecification[] {
  const byRange = new Map<string, number[]>();
  for (const [cls, rs] of ranges) {
    for (const [a, b] of rs) {
      const key = `${a.toFixed(2)}:${b.toFixed(2)}`;
      const v = byRange.get(key) ?? [];
      v.push(cls);
      byRange.set(key, v);
    }
  }
  const out: LayerSpecification[] = [];
  const keys = [...byRange.keys()].sort();
  for (const source of vectorSources(style)) {
    keys.forEach((key, i) => {
      const [a, b] = key.split(":").map(Number) as [number, number];
      const classes = byRange.get(key)!.sort((x, y) => x - y);
      out.push({
        id: `${HIT_PREFIX}areas-${source}-${i}`,
        type: "fill",
        source,
        "source-layer": "areas",
        minzoom: a,
        maxzoom: b,
        filter: ["all", NAMED, LAND_PART, ["in", ["get", "class"], ["literal", classes]]] as FilterSpecification,
        paint: { "fill-opacity": 0, "fill-color": "#000" },
      });
    });
  }
  return out;
}

/**
 * A fill coloured per country from feature-state `stat` (statistics overlay; set by stats.ts),
 * placed under the first line layer.
 */
export function statLayer(source: string): LayerSpecification {
  return {
    id: STAT_LAYER,
    type: "fill",
    source,
    "source-layer": "areas",
    // Countries are `Sovereign` areas in the bands that draw borders and `CountryRegion - <colour>`
    // areas (the Political fills) in all bands; both carry the country's entity id.
    filter: [
      "any",
      ["in", ["get", "class_name"], ["literal", ["Sovereign", "Sovereign - dependency", "Sovereign - disputed"]]],
      ["==", ["index-of", "CountryRegion", ["coalesce", ["get", "class_name"], ""]], 0],
    ],
    layout: { visibility: "none" },
    paint: {
      "fill-antialias": false,
      "fill-color": ["to-color", ["coalesce", ["feature-state", "stat"], "rgba(0,0,0,0)"]],
    },
  };
}

/** Index of the first line or symbol layer (where fills end in Encarta's draw order). */
export function firstAboveFills(style: StyleSpecification): string | undefined {
  return style.layers.find((l) => l.type === "line" || l.type === "symbol" || l.type === "circle")?.id;
}

/**
 * Per-class user overrides, as MOBB keeps them (mad-styling.md §4.3): `off` = never (u = 1),
 * `on` = also when the rule makes it optional (u = 2). Classes not listed follow the rule.
 */
export interface ClassOverrides {
  on: ReadonlySet<number>;
  off: ReadonlySet<number>;
}

export interface LayerState {
  visible: boolean;
  filter: unknown;
}

/** Visibility and filter of a style layer under the user's class overrides. */
export function layerState(layer: LayerSpecification, o: ClassOverrides): LayerState {
  const l = layer as AnyLayer;
  const { ids } = layerClasses(layer);
  const optional = isOptional(layer);
  if (!ids.length) return { visible: !optional, filter: l.filter };
  const keep = ids.filter((id) => !o.off.has(id) && (!optional || o.on.has(id)));
  if (!keep.length) return { visible: false, filter: l.filter };
  if (keep.length === ids.length) return { visible: true, filter: l.filter };
  const classFilter = keep.length === 1 ? ["==", ["get", "class"], keep[0]] : ["in", ["get", "class"], ["literal", keep]];
  // `["all", <class test>, <draw-over mask test>]`: keep the mask test (docs/atlas-output.md "Styles").
  const f = l.filter;
  if (Array.isArray(f) && f[0] === "all") return { visible: true, filter: ["all", classFilter, ...f.slice(2)] };
  return { visible: true, filter: classFilter };
}
