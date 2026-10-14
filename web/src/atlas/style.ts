/**
 * Turns a converted style into the style the map shows: URLs resolved for this client, the chosen
 * projection, the user's class overrides and label size, an optional client-side LOOK, the statistics
 * layer and the invisible hit-testing layers. Pure, so a change of options can be applied with
 * MapLibre's style diff.
 */
import type { LayerSpecification, SourceSpecification, StyleSpecification } from "maplibre-gl";
import { rasterArchive, resolveStyleUrls, type Endpoints, type Palettes } from "./data";
import { firstAboveFills, hitLayers, hitRanges, layerClasses, layerState, statLayer, vectorSources, type ClassOverrides } from "./layers";
import { millerRasterTiles, millerVectorTiles } from "./miller";
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
  /** Added to every layer's zoom range (zoom.ts `zoomShift`: pane size, latitude, flat view). */
  shift?: number;
  /** Fill and outline one entity in a colour (e.g. the Geography Quiz's country). */
  highlight?: { entity: number; color: string } | null;
}

export const HIGHLIGHT_PREFIX = "encarta-highlight-";

/** Layers that draw `entity`'s areas filled and outlined, whatever the style draws. */
export function highlightLayers(raw: StyleSpecification, entity: number, color: string): LayerSpecification[] {
  const out: LayerSpecification[] = [];
  for (const source of vectorSources(raw)) {
    out.push({
      id: `${HIGHLIGHT_PREFIX}fill-${source}`,
      type: "fill",
      source,
      "source-layer": "areas",
      filter: ["==", ["id"], entity],
      paint: { "fill-color": color, "fill-opacity": 0.6, "fill-antialias": false },
    });
    out.push({
      id: `${HIGHLIGHT_PREFIX}line-${source}`,
      type: "line",
      source,
      "source-layer": "outlines",
      filter: ["==", ["id"], entity],
      paint: { "line-color": color, "line-width": 2 },
    });
  }
  return out;
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
  /** `P/m` per pixel of pane height (VM: 6.05). */
  pOverMPerHeightPx?: number;
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
    pOverMPerHeightPx: n("encarta:view_p_over_m_per_height_px"),
    minRects: Array.isArray(m["encarta:min_rects"]) ? (m["encarta:min_rects"] as StyleMeta["minRects"]) : [],
    notes: typeof m["encarta:notes"] === "string" ? (m["encarta:notes"] as string) : undefined,
  };
}

const LABEL_LAYERS = new Set(["labels", "lines"]);

/** Default archive zoom limits when a style's vector source does not state them. */
const VECTOR_ZOOMS: Record<string, [number, number]> = { earth: [0, 8], "earth-detail": [10, 12], moon: [0, 3] };

export function buildStyle(raw: StyleSpecification, o: BuildOptions): StyleSpecification {
  const style = resolveStyleUrls(raw, o.ep);
  const flat = o.projection === "flat";
  style.projection = { type: flat ? "mercator" : "globe" };

  if (o.look && style.sources.raster && (!flat || o.look.baked)) {
    const src: SourceSpecification = o.look.baked
      ? { type: "raster", tileSize: 256, url: `pmtiles://${rasterArchive(o.look.raster, o.look.look, o.ep)}` }
      : lookSource(o.look.raster, o.look.look, o.palettes ?? {});
    style.sources = { ...style.sources, raster: src };
  }
  if (flat) style.sources = millerSources(style, raw, o);

  const layers: LayerSpecification[] = raw.layers.map((layer) => adjustLayer(layer, o));

  const vector = vectorSources(raw);
  if (vector.includes("earth")) {
    const stat = statLayer("earth");
    if (o.stats) stat.layout = { visibility: "visible" };
    const before = firstAboveFills(raw);
    const at = before ? layers.findIndex((l) => l.id === before) : layers.length;
    layers.splice(at < 0 ? layers.length : at, 0, stat);
  }
  if (o.highlight) {
    // Above the fills and lines, under the symbols.
    const at = layers.findIndex((l) => l.type === "symbol");
    layers.splice(at < 0 ? layers.length : at, 0, ...highlightLayers(raw, o.highlight.entity, o.highlight.color));
  }
  layers.push(...hitLayers(raw, hitRanges(raw, o.overrides)));
  style.layers = o.shift ? layers.map((l) => shiftLayer(l, o.shift!)) : layers;
  return style;
}

/** Moves a layer's zoom range by `shift` (layers without a range keep none). */
function shiftLayer(layer: LayerSpecification, shift: number): LayerSpecification {
  const l = layer as LayerSpecification & { minzoom?: number; maxzoom?: number };
  if (l.minzoom === undefined && l.maxzoom === undefined) return layer;
  const out = { ...l };
  if (l.minzoom !== undefined) out.minzoom = Math.min(24, Math.max(0, l.minzoom + shift));
  if (l.maxzoom !== undefined) out.maxzoom = Math.min(24, Math.max(0, l.maxzoom + shift));
  if ((out.maxzoom ?? 24) <= (out.minzoom ?? 0)) out.maxzoom = Math.min(24, (out.minzoom ?? 0) + 0.01);
  return out as LayerSpecification;
}

/** The flat view's sources: the same archives, re-tiled into Miller view coordinates (miller.ts). */
function millerSources(style: StyleSpecification, raw: StyleSpecification, o: BuildOptions): StyleSpecification["sources"] {
  const out: Record<string, SourceSpecification> = {};
  const m = styleMeta(raw);
  for (const [id, src] of Object.entries(style.sources)) {
    const s = src as { type: string; url?: string; minzoom?: number; maxzoom?: number };
    const archive = s.url?.startsWith("pmtiles://") ? s.url.slice("pmtiles://".length) : undefined;
    if (s.type === "vector" && archive) {
      const [lo, hi] = [s.minzoom ?? VECTOR_ZOOMS[id]?.[0] ?? 0, s.maxzoom ?? VECTOR_ZOOMS[id]?.[1] ?? 12];
      out[id] = { type: "vector", tiles: [millerVectorTiles(archive, hi)], minzoom: lo, maxzoom: hi };
    } else if (id === "raster" && archive && m.raster) {
      const look = o.look?.baked ? o.look.look : (m.look ?? "");
      const maxLevel = o.palettes?.[m.raster.toUpperCase()]?.max_level;
      out[id] = { type: "raster", tileSize: 256, tiles: [millerRasterTiles(m.raster, look, archive)], minzoom: 0, maxzoom: maxLevel !== undefined ? maxLevel + 1 : 9 };
    } else if (id !== "raster-index") out[id] = src as SourceSpecification;
  }
  return out;
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
