/**
 * Turns a converted style into the style the map shows: URLs resolved for this client, the chosen
 * projection, the user's class overrides and label size, an optional client-side LOOK, the statistics
 * layer and the invisible hit-testing layers. Pure, so a change of options can be applied with
 * MapLibre's style diff.
 */
import type { LayerSpecification, SourceSpecification, StyleSpecification } from "maplibre-gl";
import { rasterArchive, resolveStyleUrls, type Endpoints, type Palettes } from "./data";
import { firstAboveFills, hitLayers, layerClasses, layerState, statLayer, vectorSources, type ClassOverrides } from "./layers";
import { lookSource } from "./recolour";

export type Projection = "globe" | "flat";

export interface BuildOptions {
  ep: Endpoints;
  projection: Projection;
  overrides: ClassOverrides;
  /** 1 = normal, 1.25 = large, 0 = no labels (symbols stay). */
  labelScale: number;
  /** Draw this LOOK of the style's raster instead of its own. `baked` = an archive of it exists. */
  look?: { raster: string; look: string; baked: boolean } | null;
  palettes?: Palettes | null;
  /** Show the statistics fill layer. */
  stats?: boolean;
}

export interface StyleMeta {
  styleId?: number;
  raster?: string;
  look?: string;
  minzoom: number;
  maxzoom: number;
  minAltitudeKm?: number;
  maxAltitudeKm?: number;
  spaceColour: string;
  pOverM?: number;
  minRects: { bbox: [number, number, number, number]; maxzoom: number; min_altitude_km?: number }[];
  notes?: string;
}

export function styleMeta(style: StyleSpecification): StyleMeta {
  const m = (style.metadata ?? {}) as Record<string, unknown>;
  const n = (k: string) => (typeof m[k] === "number" ? (m[k] as number) : undefined);
  return {
    styleId: n("encarta:style_id"),
    raster: typeof m["encarta:raster"] === "string" ? (m["encarta:raster"] as string) : undefined,
    look: typeof m["encarta:look"] === "string" ? (m["encarta:look"] as string) : undefined,
    minzoom: Math.max(0, n("encarta:minzoom") ?? 0),
    maxzoom: n("encarta:maxzoom") ?? 22,
    minAltitudeKm: n("encarta:min_altitude_km"),
    maxAltitudeKm: n("encarta:max_altitude_km"),
    spaceColour: typeof m["encarta:space_colour"] === "string" ? (m["encarta:space_colour"] as string) : "#000000",
    pOverM: n("encarta:view_p_over_m"),
    minRects: Array.isArray(m["encarta:min_rects"]) ? (m["encarta:min_rects"] as StyleMeta["minRects"]) : [],
    notes: typeof m["encarta:notes"] === "string" ? (m["encarta:notes"] as string) : undefined,
  };
}

const LABEL_LAYERS = new Set(["labels", "lines"]);

export function buildStyle(raw: StyleSpecification, o: BuildOptions): StyleSpecification {
  const style = resolveStyleUrls(raw, o.ep);
  style.projection = { type: o.projection === "globe" ? "globe" : "mercator" };

  if (o.look && style.sources.raster) {
    const src: SourceSpecification = o.look.baked
      ? { type: "raster", tileSize: 256, url: `pmtiles://${rasterArchive(o.look.raster, o.look.look, o.ep)}` }
      : lookSource(o.look.raster, o.look.look, o.palettes ?? {});
    style.sources = { ...style.sources, raster: src };
  }

  const layers: LayerSpecification[] = raw.layers.map((layer) => adjustLayer(layer, o));

  const vector = vectorSources(raw);
  if (vector.includes("earth")) {
    const stat = statLayer("earth");
    if (o.stats) stat.layout = { visibility: "visible" };
    const before = firstAboveFills(raw);
    const at = before ? layers.findIndex((l) => l.id === before) : layers.length;
    layers.splice(at < 0 ? layers.length : at, 0, stat);
  }
  layers.push(...hitLayers(raw));
  style.layers = layers;
  return style;
}

function adjustLayer(layer: LayerSpecification, o: BuildOptions): LayerSpecification {
  if (layer.type === "background" || layer.type === "raster") return layer;
  const state = layerState(layer, o.overrides);
  const l = { ...layer } as LayerSpecification & { layout?: Record<string, unknown>; paint?: Record<string, unknown>; filter?: unknown };
  const layout = { ...(l.layout ?? {}) };
  const paint = { ...(l.paint ?? {}) };
  layout.visibility = state.visible ? "visible" : "none";
  if (state.filter !== undefined) (l as { filter?: unknown }).filter = state.filter;

  if (layer.type === "symbol") {
    const sourceLayer = (layer as { "source-layer"?: string })["source-layer"] ?? "";
    // An optional label on a shown symbol starts hidden (text-opacity 0); turning its class on shows it.
    if (paint["text-opacity"] === 0 && layerClasses(layer).ids.some((id) => o.overrides.on.has(id))) paint["text-opacity"] = 1;
    // The Moon's maria carry their English name as the alternative (U+0004); Encarta shows it on a
    // second line ("Mare Frigoris / Sea of Cold", V4 Moon shot).
    if ((layer as { source?: string }).source === "moon" && sourceLayer === "labels" && layout["text-field"] !== undefined) {
      layout["text-field"] = [
        "case",
        ["has", "alt_names"],
        ["format", ["coalesce", ["get", "name"], ""], {}, "\n", {}, ["get", "alt_names"], { "font-scale": 0.85 }],
        ["coalesce", ["get", "name"], ""],
      ];
    }
    if (o.labelScale === 0) {
      if (LABEL_LAYERS.has(sourceLayer) || layout["icon-image"] === undefined) layout.visibility = "none";
      else delete layout["text-field"];
    } else if (o.labelScale !== 1 && typeof layout["text-size"] === "number") {
      layout["text-size"] = Math.round((layout["text-size"] as number) * o.labelScale * 10) / 10;
    }
  }
  l.layout = layout as never;
  l.paint = paint as never;
  return l;
}

/** Class rows that some layer shows by default (not only as an option). */
export function defaultVisibleClasses(raw: StyleSpecification): Set<number> {
  const out = new Set<number>();
  for (const layer of raw.layers) {
    const vis = (layer as { layout?: { visibility?: string } }).layout?.visibility;
    if (vis === "none") continue;
    for (const id of layerClasses(layer).ids) out.add(id);
  }
  return out;
}
