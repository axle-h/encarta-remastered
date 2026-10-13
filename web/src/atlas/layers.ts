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

/** Five-letter prefixes of thematic area classes that cover the world (languages, religions, climate …). */
export const THEME_PREFIXES = ["LANG ", "REL -", "CLI -", "ECO -", "Ecore", "TECT ", "Time ", "Dayli", "World"];

function namedFilter(drawn: readonly number[]): FilterSpecification {
  return [
    "all",
    ["has", "name"],
    ["!=", ["get", "name"], ""],
    ["!", ["to-boolean", ["coalesce", ["get", "anonymous"], false]]],
    // Membership areas (one per organisation, covering its member states) are not places; Encarta's
    // click menu does not list them (V11).
    ["!=", ["get", "class_name"], "International organization"],
    // World-wide thematic areas only where the style draws them: elsewhere they are not in Encarta's
    // menu, and at low zooms their huge polygons made the invisible layer freeze the page.
    [
      "any",
      ["!", ["in", ["slice", ["coalesce", ["get", "class_name"], ""], 0, 5], ["literal", THEME_PREFIXES]]],
      ["in", ["get", "class"], ["literal", [...drawn]]],
    ],
  ] as FilterSpecification;
}

/**
 * Invisible layers for hit-testing: the named features of the `areas` layer, so a click can list
 * everything under the cursor, as Encarta's pop-up menu does (V11 shot), including label-only areas
 * that no layer draws (regions, basins). MapLibre queries layers by geometry, not by opacity. Lines
 * are hit-tested on their drawn layers only: an invisible 7-px line layer over every named line
 * stalled the page at world zooms.
 * `drawn` = class rows the style shows by default.
 */
export function hitLayers(style: StyleSpecification, drawn: readonly number[] = []): LayerSpecification[] {
  const filter = namedFilter(drawn);
  const out: LayerSpecification[] = [];
  for (const source of vectorSources(style)) {
    out.push({
      id: `${HIT_PREFIX}areas-${source}`,
      type: "fill",
      source,
      "source-layer": "areas",
      filter,
      paint: { "fill-opacity": 0, "fill-color": "#000" },
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
  return { visible: true, filter: classFilter };
}
