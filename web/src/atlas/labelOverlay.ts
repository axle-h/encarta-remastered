/**
 * Encarta's label placement on the map (placement.ts, mad-styling.md §7.3, §7.4, §8.1). MapLibre cannot
 * weigh candidate costs, place one shield per road run or fall back to another text layout, so the
 * point, area and shield labels are placed here when the map comes to rest, and drawn by MapLibre from a
 * GeoJSON source with each label's own text lines, anchor and offset (`overlayStyle` rewrites their
 * layers onto it). Line names along the line stay MapLibre's and are placed after these.
 */
import maplibregl, { type GeoJSONSource, type LayerSpecification, type Map as MlMap, type StyleSpecification } from "maplibre-gl";
import { decodePngValues } from "./png";
import { GlyphMetrics, LINE_HEIGHT } from "./glyphMetrics";
import { isCjk, labelLayouts, type LabelTexts } from "./labelText";
import { clipLine, joinRuns, placeLabels, type Box, type ClassCounts, type Label, type Placement, type Position, type SizedLayout, type TextLabel } from "./placement";
import { PMTiles } from "pmtiles";

export const OVERLAY_SOURCE = "encarta-labels";

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

/** What the placement needs to know about one rewritten layer. */
export interface OverlaySpec {
  id: string;
  kind: "point" | "area" | "shield";
  source: string;
  sourceLayer: string;
  filter: unknown;
  minzoom: number;
  maxzoom: number;
  visible: boolean;
  /** Text font stack, size (px), letter spacing (em) and upper case. No font: a symbol without a name. */
  font?: string;
  size: number;
  spacing: number;
  upper: boolean;
  /** The alternative-name line (`w6`): glyph faces with the rule, scale, upper case. */
  alt?: { mainRuleFont: string; font: string; scale: number; upper: boolean };
  icon?: string;
  /** Constant priority (b13) of layers whose features carry none (shields). */
  priority?: number;
}

const isShieldLayer = (l: AnyLayer) => l["source-layer"] === "lines" && l.layout?.["icon-text-fit"] !== undefined;

/** The layers the placement takes over: point and area labels (and bare symbols) and road shields. */
export function overlayKind(layer: LayerSpecification): OverlaySpec["kind"] | null {
  const l = layer as AnyLayer;
  if (l.type !== "symbol") return null;
  if (l["source-layer"] === "points") return "point";
  if (l["source-layer"] === "labels") return "area";
  if (isShieldLayer(l)) return "shield";
  return null;
}

/** The faces of the alternative-name `format` expression (style.rs `with_alt`): main rule face, then the alternative's. */
function altFormat(field: unknown): { mainRuleFont: string; font: string; scale: number; colour: string; upper: boolean } | undefined {
  const json = JSON.stringify(field ?? null);
  if (!json.includes('"format"') || !json.includes("alt_names")) return undefined;
  const sections: { font?: string; scale?: number; colour?: string }[] = [];
  const walk = (v: unknown) => {
    if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") {
      const o = v as Record<string, unknown>;
      const f = o["text-font"];
      if (Array.isArray(f) && f[0] === "literal" && Array.isArray(f[1])) sections.push({ font: String(f[1][0]), scale: o["font-scale"] as number | undefined, colour: o["text-color"] as string | undefined });
    }
  };
  walk(field);
  if (sections.length < 2) return undefined;
  return {
    mainRuleFont: sections[0]!.font!,
    font: sections[1]!.font!,
    scale: typeof sections[1]!.scale === "number" ? sections[1]!.scale : 1,
    colour: sections[1]!.colour ?? "#000000",
    upper: json.includes('["upcase",["get","alt_names"]]'),
  };
}

function specOf(layer: AnyLayer, kind: OverlaySpec["kind"]): OverlaySpec {
  const layout = layer.layout ?? {};
  const paint = layer.paint ?? {};
  const field = layout["text-field"];
  const fieldJson = JSON.stringify(field ?? null);
  const fonts = layout["text-font"];
  const hasText = field !== undefined && paint["text-opacity"] !== 0;
  const alt = altFormat(field);
  const sortKey = layout["symbol-sort-key"];
  return {
    id: layer.id,
    kind,
    source: layer.source ?? "",
    sourceLayer: layer["source-layer"] ?? "",
    filter: layer.filter,
    minzoom: layer.minzoom ?? 0,
    maxzoom: layer.maxzoom ?? 24,
    visible: layout.visibility !== "none",
    font: hasText && Array.isArray(fonts) ? String(fonts[0]) : undefined,
    size: typeof layout["text-size"] === "number" ? layout["text-size"] : 12,
    spacing: typeof layout["text-letter-spacing"] === "number" ? layout["text-letter-spacing"] : 0,
    upper: fieldJson.includes('["upcase",["coalesce",["get","name"]'),
    alt: alt && { mainRuleFont: alt.mainRuleFont, font: alt.font, scale: alt.scale, upper: alt.upper },
    icon: typeof layout["icon-image"] === "string" ? layout["icon-image"] : undefined,
    priority: typeof sortKey === "number" ? sortKey : undefined,
  };
}

/**
 * Rewrites the point, area and shield label layers of a built style onto the overlay source: same ids,
 * fonts, colours and zoom ranges; text, anchor and offset per feature; everything placed (the
 * placement has tested the collisions) and still in MapLibre's collision index, so that the line names
 * MapLibre places afterwards keep clear. Line-name layers move under the overlay layers so that they
 * are placed last.
 */
export function overlayStyle(style: StyleSpecification): { style: StyleSpecification; specs: OverlaySpec[] } {
  const specs: OverlaySpec[] = [];
  const lineNames: LayerSpecification[] = [];
  const rest: LayerSpecification[] = [];
  for (const layer of style.layers) {
    const l = layer as AnyLayer;
    const kind = overlayKind(layer);
    if (!kind) {
      if (l.type === "symbol" && l["source-layer"] === "lines") lineNames.push(layer);
      else rest.push(layer);
      continue;
    }
    const spec = specOf(l, kind);
    specs.push(spec);
    rest.push(overlayLayer(l, spec));
  }
  const firstOverlay = rest.findIndex((l) => (l as AnyLayer).source === OVERLAY_SOURCE);
  const layers = firstOverlay < 0 ? [...rest, ...lineNames] : [...rest.slice(0, firstOverlay), ...lineNames, ...rest.slice(firstOverlay)];
  return {
    style: {
      ...style,
      sources: { ...style.sources, [OVERLAY_SOURCE]: { type: "geojson", data: { type: "FeatureCollection", features: [] }, tolerance: 0, buffer: 128 } },
      layers,
    },
    specs,
  };
}

function overlayLayer(l: AnyLayer, spec: OverlaySpec): LayerSpecification {
  const layout: Record<string, unknown> = { ...(l.layout ?? {}) };
  const paint: Record<string, unknown> = { ...(l.paint ?? {}) };
  for (const k of ["text-variable-anchor", "text-radial-offset", "symbol-spacing", "text-max-width", "symbol-placement"]) delete layout[k];
  layout["symbol-sort-key"] = ["get", "_order"];
  layout["text-allow-overlap"] = true;
  layout["icon-allow-overlap"] = true;
  layout["text-ignore-placement"] = false;
  layout["icon-ignore-placement"] = false;
  layout["icon-optional"] = true;
  if (spec.kind === "shield") {
    layout["text-field"] = ["get", "_text"];
    layout["text-anchor"] = "center";
  } else if (layout["text-field"] !== undefined) {
    layout["text-field"] = textField(l, spec);
    layout["text-anchor"] = ["get", "_anchor"];
    // GeoJSON properties reach the expressions flattened (arrays as strings), so the offset is built
    // from the position and one distance in ems: interpolation is linear on number arrays.
    const along = (dx: number, dy: number) => ["interpolate", ["linear"], ["get", "_d"], 0, ["literal", [0, 0]], 100, ["literal", [100 * dx, 100 * dy]]];
    layout["text-offset"] = ["match", ["get", "_pos"], "above-right", along(1, 0), "below-right", along(1, 1), "above-left", along(-1, 0), "below-left", along(-1, 1), ["literal", [0, 0]]];
    layout["text-justify"] = ["get", "_justify"];
    layout["text-max-width"] = 1000;
    if (paint["text-opacity"] === undefined) paint["text-opacity"] = ["case", ["==", ["get", "_text_on"], true], 1, 0];
  }
  if (spec.icon) paint["icon-opacity"] = ["case", ["==", ["get", "_icon"], false], 0, 1];
  const out: AnyLayer = { ...l, source: OVERLAY_SOURCE, filter: ["==", ["get", "_layer"], l.id], layout, paint } as AnyLayer;
  delete out["source-layer"];
  return out as LayerSpecification;
}

/** Text of a rewritten label: `_text`, or with an alternative `_head` + `_last` (ruled) over `_alt`. */
function textField(l: AnyLayer, spec: OverlaySpec): unknown {
  const font = spec.font ?? String((l.layout?.["text-font"] as string[] | undefined)?.[0] ?? "");
  const alt = altFormat(l.layout?.["text-field"]);
  if (!alt) return ["get", "_text"];
  const tailSections = [
    ["get", "_last"],
    { "text-font": ["literal", [alt.mainRuleFont]] },
    "\n",
    {},
    ["get", "_alt"],
    { "text-font": ["literal", [alt.font]], "font-scale": alt.scale, "text-color": alt.colour },
  ];
  return [
    "case",
    ["!", ["has", "_alt"]],
    ["get", "_text"],
    ["==", ["get", "_head"], ""],
    ["format", ...tailSections],
    ["format", ["get", "_head"], { "text-font": ["literal", [font]] }, ...tailSections],
  ];
}

// ---------------------------------------------------------------------------------------------
// Feature filters (the converted styles use class tests and `has`)

type Props = Record<string, unknown>;

export function evalFilter(f: unknown, p: Props): boolean {
  return Boolean(evalExpr(f, p));
}

function evalExpr(e: unknown, p: Props): unknown {
  if (!Array.isArray(e)) return e;
  const [op, ...a] = e as unknown[];
  switch (op) {
    case "get":
      return p[a[0] as string];
    case "has":
      return p[a[0] as string] !== undefined && p[a[0] as string] !== null;
    case "!has":
      return !(p[a[0] as string] !== undefined && p[a[0] as string] !== null);
    case "literal":
      return a[0];
    case "!":
      return !evalExpr(a[0], p);
    case "all":
      return a.every((x) => evalExpr(x, p));
    case "any":
      return a.some((x) => evalExpr(x, p));
    case "==":
      return evalExpr(a[0], p) === evalExpr(a[1], p);
    case "!=":
      return evalExpr(a[0], p) !== evalExpr(a[1], p);
    case "in": {
      const v = evalExpr(a[0], p);
      const list = evalExpr(a[1], p);
      return Array.isArray(list) ? list.includes(v) : typeof list === "string" && typeof v === "string" ? list.includes(v) : false;
    }
    case "coalesce":
      for (const x of a) {
        const v = evalExpr(x, p);
        if (v !== undefined && v !== null) return v;
      }
      return null;
    default:
      return true; // unknown: do not filter
  }
}

// ---------------------------------------------------------------------------------------------
// Pixel classes of the view (§5.2) for the background cost

/** Pixel class of a raster value with CMT `kind` (palettes.json `pixel_class` holds the kind). */
export function classOfKind(kind: number): number {
  return kind === 0 ? 1 : kind === 1 ? 2 : kind === 2 ? 4 : kind === 3 ? 5 : 0;
}

/** A coarse grid of the pane's pixel classes with summed-area tables, for O(1) counts under a box. */
export class ClassGrid {
  readonly cols: number;
  readonly rows: number;
  private water: Float64Array;
  private land: Float64Array;
  private marked: Float64Array;
  private cellClass: Uint8Array;
  private lineArea: Float32Array;
  constructor(
    readonly width: number,
    readonly height: number,
    readonly cell = 4,
  ) {
    this.cols = Math.ceil(width / cell);
    this.rows = Math.ceil(height / cell);
    this.cellClass = new Uint8Array(this.cols * this.rows).fill(4);
    this.lineArea = new Float32Array(this.cols * this.rows);
    const n = (this.cols + 1) * (this.rows + 1);
    this.water = new Float64Array(n);
    this.land = new Float64Array(n);
    this.marked = new Float64Array(n);
  }

  setClass(col: number, row: number, cls: number): void {
    this.cellClass[row * this.cols + col] = cls;
  }

  /** Adds a drawn line (class 6 pixels) of `width` px along a screen segment. */
  addSegment(ax: number, ay: number, bx: number, by: number, width: number): void {
    const len = Math.hypot(bx - ax, by - ay);
    const steps = Math.max(1, Math.ceil(len / (this.cell / 2)));
    const per = (len / steps) * width;
    for (let i = 0; i < steps; i++) {
      const t = (i + 0.5) / steps;
      const c = Math.floor((ax + (bx - ax) * t) / this.cell);
      const r = Math.floor((ay + (by - ay) * t) / this.cell);
      if (c >= 0 && r >= 0 && c < this.cols && r < this.rows) {
        const i = r * this.cols + c;
        this.lineArea[i] = this.lineArea[i]! + per;
      }
    }
  }

  /** Builds the summed-area tables; call after the classes and lines are in. */
  finish(): void {
    const { cols, rows } = this;
    const area = this.cell * this.cell;
    const W = cols + 1;
    for (let r = 0; r < rows; r++) {
      let w = 0;
      let l = 0;
      let m = 0;
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        const line = Math.min(area, this.lineArea[i]!);
        const cls = this.cellClass[i]!;
        const rest = area - line;
        if (cls >= 1 && cls <= 3) w += rest;
        else if (cls === 4) l += rest;
        else if (cls === 0 || cls === 6 || cls === 8) m += rest;
        m += line;
        const o = (r + 1) * W + (c + 1);
        this.water[o] = this.water[o - W]! + w;
        this.land[o] = this.land[o - W]! + l;
        this.marked[o] = this.marked[o - W]! + m;
      }
    }
  }

  private sum(t: Float64Array, c0: number, r0: number, c1: number, r1: number): number {
    const W = this.cols + 1;
    return t[r1 * W + c1]! - t[r0 * W + c1]! - t[r1 * W + c0]! + t[r0 * W + c0]!;
  }

  counts(b: Box): ClassCounts {
    const c0 = Math.max(0, Math.min(this.cols, Math.floor(b.x0 / this.cell)));
    const c1 = Math.max(0, Math.min(this.cols, Math.ceil(b.x1 / this.cell)));
    const r0 = Math.max(0, Math.min(this.rows, Math.floor(b.y0 / this.cell)));
    const r1 = Math.max(0, Math.min(this.rows, Math.ceil(b.y1 / this.cell)));
    const water = this.sum(this.water, c0, r0, c1, r1);
    const land = this.sum(this.land, c0, r0, c1, r1);
    const marked = this.sum(this.marked, c0, r0, c1, r1);
    return { water, land, marked, total: (c1 - c0) * (r1 - r0) * this.cell * this.cell };
  }
}

// ---------------------------------------------------------------------------------------------
// The placement pass

export interface PlacerOptions {
  /** Real latitude of a map latitude (the flat view draws in view latitudes). */
  fromMapLat: (lat: number) => number;
  /** The style raster's `index` archive and the CMT kind of each value (palettes.json), for the background. */
  raster?: { archive: string; kinds: number[]; maxZoom: number };
  /** Entity placed first (the selected place). */
  selected?: () => number | null;
}

interface TileFeature {
  id?: string | number;
  properties: Props;
  _vectorTileFeature?: { extent: number; type: number; loadGeometry(): { x: number; y: number }[][] };
  _x?: number;
  _y?: number;
  _z?: number;
  geometry: { type: string; coordinates: unknown };
}

function tileLines(f: TileFeature): [number, number][][] {
  const vt = f._vectorTileFeature;
  if (!vt || f._z === undefined || f._x === undefined || f._y === undefined) {
    const g = f.geometry;
    if (g.type === "LineString") return [g.coordinates as [number, number][]];
    if (g.type === "MultiLineString") return g.coordinates as [number, number][][];
    return [];
  }
  const size = vt.extent * 2 ** f._z;
  const x0 = vt.extent * f._x;
  const y0 = vt.extent * f._y;
  const out: [number, number][][] = [];
  for (const ring of vt.loadGeometry()) {
    // Only the part inside the tile itself (not its buffer), so tile pieces meet end to end.
    for (const piece of clipLine(
      ring.map((p) => [p.x, p.y] as [number, number]),
      { x0: 0, y0: 0, x1: vt.extent, y1: vt.extent },
    )) {
      out.push(piece.map(([x, y]) => [((x + x0) * 360) / size - 180, (360 / Math.PI) * Math.atan(Math.exp((1 - ((y + y0) * 2) / size) * Math.PI)) - 90]));
    }
  }
  return out;
}

function pointsOf(f: TileFeature): [number, number][] {
  const g = f.geometry;
  if (g.type === "Point") return [g.coordinates as [number, number]];
  if (g.type === "MultiPoint") return g.coordinates as [number, number][];
  return [];
}

const upper = (s: string) => s.toLocaleUpperCase().replace(/ß/g, "SS");

/**
 * Anchor, offset and justification that make MapLibre draw a text box where the placement put it. The
 * offset is `d` ems along the position's direction (right/left, and down for the lower ones).
 */
export function anchorFor(pos: Position, r: number, size: number, cjk: boolean): { anchor: string; d: number; justify: string } {
  const e = (px: number) => Math.round((px / size) * 1000) / 1000;
  const side = (j: string) => (cjk ? "center" : j);
  switch (pos) {
    case "center":
      return { anchor: "center", d: 0, justify: "center" };
    case "above-right":
      return { anchor: "bottom-left", d: e(r + 1), justify: side("left") };
    case "below-right":
      return { anchor: "top-left", d: e(r + 1), justify: side("left") };
    case "above-left":
      return { anchor: "bottom-right", d: e(r + 1), justify: side("right") };
    case "below-left":
      return { anchor: "top-right", d: e(r), justify: side("right") };
  }
}

export class LabelPlacer {
  private metrics: GlyphMetrics;
  private archive?: PMTiles;
  private tiles = new Map<string, Promise<{ values: Uint8Array; width: number; height: number } | null>>();
  private dirty = true;
  private running = false;
  private lastKey = "";
  private disposed = false;
  private off: (() => void)[] = [];
  /** The last placement's duration and label count (dev page, tests). */
  stats = { ms: 0, labels: 0 };

  constructor(
    private map: MlMap,
    private specs: OverlaySpec[],
    glyphs: string,
    private opts: PlacerOptions,
  ) {
    this.metrics = new GlyphMetrics(glyphs);
    if (opts.raster) this.archive = new PMTiles(opts.raster.archive);
    const mark = () => {
      this.dirty = true;
    };
    const onData = (e: { sourceId?: string; tile?: unknown }) => {
      if (e.sourceId && e.sourceId !== OVERLAY_SOURCE && e.tile) this.dirty = true;
    };
    const onIdle = () => void this.run();
    const onStyle = () => {
      this.dirty = true;
      this.lastKey = "";
    };
    map.on("moveend", mark);
    map.on("sourcedata", onData);
    map.on("idle", onIdle);
    map.on("style.load", onStyle);
    this.off.push(() => map.off("moveend", mark), () => map.off("sourcedata", onData), () => map.off("idle", onIdle), () => map.off("style.load", onStyle));
  }

  /** New layer specs (style options changed): place again. */
  update(specs: OverlaySpec[]): void {
    this.specs = specs;
    this.dirty = true;
    this.lastKey = "";
  }

  invalidate(): void {
    this.dirty = true;
  }

  dispose(): void {
    this.disposed = true;
    for (const f of this.off) f();
  }

  private async run(): Promise<void> {
    if (!this.dirty || this.running || this.disposed) return;
    this.running = true;
    this.dirty = false;
    try {
      const t0 = performance.now();
      const data = await this.place();
      if (this.disposed || !data) return;
      this.stats = { ms: Math.round(performance.now() - t0), labels: data.features.length };
      const key = JSON.stringify(data.features.map((f) => [f.properties._layer, f.properties._anchor, f.properties._text, f.geometry.coordinates]));
      if (key === this.lastKey) return;
      this.lastKey = key;
      (this.map.getSource(OVERLAY_SOURCE) as GeoJSONSource | undefined)?.setData(data);
    } catch (e) {
      console.warn("atlas labels:", e);
    } finally {
      this.running = false;
    }
  }

  /** One placement of the view as it is now. Exposed for tests and the dev page. */
  async place(): Promise<{ type: "FeatureCollection"; features: { type: "Feature"; id?: number; geometry: { type: "Point"; coordinates: [number, number] }; properties: Props }[] } | null> {
    const map = this.map;
    if (!map.getSource(OVERLAY_SOURCE)) return null;
    const canvas = map.getCanvas();
    const W = canvas.clientWidth;
    const H = canvas.clientHeight;
    const z = map.getZoom();
    const globe = map.getProjection()?.type === "globe";
    const transform = (map as unknown as { transform: { isLocationOccluded?(l: maplibregl.LngLat): boolean; isPointOnMapSurface?(p: maplibregl.Point): boolean } }).transform;
    const onSurface = (x: number, y: number) => !globe || !transform.isPointOnMapSurface || transform.isPointOnMapSurface(new maplibregl.Point(x, y));
    const inside = (b: Box) => b.x0 >= 0 && b.y0 >= 0 && b.x1 <= W && b.y1 <= H && (!globe || (onSurface(b.x0, b.y0) && onSurface(b.x1, b.y0) && onSurface(b.x0, b.y1) && onSurface(b.x1, b.y1)));
    const visible = (lng: number, lat: number) => !globe || !transform.isLocationOccluded || !transform.isLocationOccluded(new maplibregl.LngLat(lng, lat));
    const active = this.specs.filter((s) => s.visible && z >= s.minzoom && z < s.maxzoom);

    // Features by (source, source layer), once each.
    const bySource = new Map<string, TileFeature[]>();
    const featuresOf = (source: string, sourceLayer: string): TileFeature[] => {
      const k = `${source}/${sourceLayer}`;
      let fs = bySource.get(k);
      if (!fs) {
        fs = map.getSource(source) ? (map.querySourceFeatures(source, { sourceLayer }) as unknown as TileFeature[]) : [];
        bySource.set(k, fs);
      }
      return fs;
    };

    // Texts to measure: load their glyph ranges first.
    const pending: Promise<void>[] = [];
    const labels: Label[] = [];
    const meta = new Map<Label, { spec: OverlaySpec; f: TileFeature; lngLat: [number, number]; r: number; texts?: LabelTexts }>();
    const selected = this.opts.selected?.() ?? null;
    const iconSize = (name: string | undefined) => {
      if (!name || !map.hasImage(name)) return 0;
      const img = map.getImage(name);
      return img ? Math.max(img.data.width, img.data.height) / (img.pixelRatio || 1) : 0;
    };
    const seen = new Set<string>();
    const margin = 200;
    const roads = new Map<string, { spec: OverlaySpec; f: TileFeature; order: Label["order"]; text: string; features: TileFeature[] }>();

    for (const spec of active) {
      const symbol = spec.kind === "shield" ? 0 : iconSize(spec.icon);
      for (const f of featuresOf(spec.source, spec.sourceLayer)) {
        if (!evalFilter(spec.filter, f.properties)) continue;
        const p = f.properties;
        const entity = typeof f.id === "number" ? f.id : Number(p.entity ?? 0);
        const order = {
          flagged: selected !== null && entity === selected,
          priority: typeof p.priority === "number" ? p.priority : (spec.priority ?? 255),
          icon: 65535,
          geoobj: typeof p.geoobj === "number" ? p.geoobj : 0,
          entity,
        };
        const name = typeof p.name === "string" ? p.name : "";
        if (spec.kind === "shield") {
          if (!name || !spec.font) continue;
          const key = `${spec.id}|${entity}|${p.class}`;
          const road = roads.get(key);
          if (road) road.features.push(f);
          else roads.set(key, { spec, f, order, text: spec.upper ? upper(name) : name, features: [f] });
          continue;
        }
        for (const [lng, lat] of pointsOf(f)) {
          const q = map.project([lng, lat]);
          if (q.x < -margin || q.y < -margin || q.x > W + margin || q.y > H + margin || !visible(lng, lat)) continue;
          const key = `${spec.id}|${entity}|${Math.round(q.x)}|${Math.round(q.y)}`;
          if (seen.has(key)) continue;
          seen.add(key);
          let texts: LabelTexts | undefined;
          if (spec.font && name && !p.anonymous) {
            const alt = spec.alt && typeof p.alt_names === "string" && p.alt_names ? p.alt_names : undefined;
            const altText = alt === undefined ? undefined : isCjk(name) ? `(${alt})` : alt;
            const abbr = typeof p.abbr === "string" && p.abbr ? p.abbr : undefined;
            texts = {
              main: spec.upper ? upper(name) : name,
              alt: altText === undefined ? undefined : spec.alt!.upper ? upper(altText) : altText,
              abbr: abbr && (spec.upper ? upper(abbr) : abbr),
            };
            pending.push(this.metrics.ensure(spec.font, `${texts.main}${texts.abbr ?? ""}X`));
            if (spec.alt && texts.alt) pending.push(this.metrics.ensure(spec.alt.mainRuleFont, texts.main), this.metrics.ensure(spec.alt.font, texts.alt));
          }
          const l: TextLabel = { kind: spec.kind === "area" ? "area" : "point", x: q.x, y: q.y, symbol, groups: [], order };
          labels.push(l);
          meta.set(l, { spec, f, lngLat: [lng, lat], r: symbol, texts });
        }
      }
    }
    // Shield roads: every tile piece of a road, on screen and clipped to the pane, joined into runs.
    for (const road of roads.values()) {
      const pieces: [number, number][][] = [];
      for (const f of road.features) {
        for (const line of tileLines(f)) {
          const scr = line.map(([lng, lat]) => {
            const q = map.project([lng, lat]);
            return [q.x, q.y] as [number, number];
          });
          pieces.push(...clipLine(scr, { x0: 0, y0: 0, x1: W, y1: H }));
        }
      }
      if (!pieces.length) continue;
      pending.push(this.metrics.ensure(road.spec.font!, road.text));
      for (const run of joinRuns(pieces)) {
        const l: Label = { kind: "shield", line: run, w: 0, h: 0, text: road.text, order: road.order };
        labels.push(l);
        meta.set(l, { spec: road.spec, f: road.f, lngLat: [0, 0], r: 0 });
      }
    }
    const classes = this.archive ? this.classGrid(W, H, z) : Promise.resolve(null);
    await Promise.all(pending);
    const grid = await classes;
    if (this.disposed) return null;

    // Measure: layouts with their boxes; shields' boxes.
    for (const l of labels) {
      const m = meta.get(l)!;
      const spec = m.spec;
      if (l.kind === "shield") {
        l.w = this.metrics.width(spec.font!, l.text, spec.size, spec.spacing) + 6;
        l.h = spec.size * LINE_HEIGHT + 2;
        continue;
      }
      if (!m.texts) continue;
      const lh = spec.size * LINE_HEIGHT;
      l.groups = labelLayouts(m.texts).map((g) =>
        g.map((layout): SizedLayout => {
          const widths = layout.lines.map((line, i) =>
            this.metrics.width(layout.alt !== undefined && i === layout.lines.length - 1 && spec.alt ? spec.alt.mainRuleFont : spec.font!, line, spec.size, spec.spacing),
          );
          let w = Math.max(...widths);
          let h = layout.lines.length * lh;
          if (layout.alt !== undefined && spec.alt) {
            w = Math.max(w, this.metrics.width(spec.alt.font, layout.alt, spec.size * spec.alt.scale, spec.spacing));
            h += lh * Math.max(1, spec.alt.scale);
          }
          return { ...layout, w, h };
        }),
      );
      const wz = m.f.properties.width_z10;
      if (l.kind === "area" && typeof wz === "number" && m.texts.abbr) l.maxWidth = wz * 2 ** (z - 10);
      if (l.kind === "point") l.reserve = { w: 4 * this.metrics.advance(spec.font!, 88, spec.size), h: 2 * l.symbol };
    }

    const placed = placeLabels(labels, { width: W, height: H, inside, classes: grid ? (b) => grid.counts(b) : undefined });
    return { type: "FeatureCollection", features: placed.map((p, i) => this.feature(p, i, meta)) };
  }

  private feature(p: Placement, i: number, meta: Map<Label, { spec: OverlaySpec; f: TileFeature; lngLat: [number, number]; r: number }>) {
    const m = meta.get(p.label)!;
    const props: Props = { ...m.f.properties, _layer: m.spec.id, _sl: m.spec.sourceLayer, _order: i };
    const id = typeof m.f.id === "number" ? m.f.id : undefined;
    if (p.kind === "shield") {
      props._text = p.label.text;
      const ll = this.map.unproject([p.x, p.y]);
      return { type: "Feature" as const, id, geometry: { type: "Point" as const, coordinates: [ll.lng, ll.lat] as [number, number] }, properties: props };
    }
    props._icon = p.icon;
    props._text_on = !!p.layout;
    if (p.layout) {
      const spec = m.spec;
      const r = p.icon ? Math.floor(p.label.symbol / 2) : 0;
      const a = anchorFor(p.pos!, r, spec.size, isCjk(p.layout.lines[0] ?? ""));
      props._anchor = a.anchor;
      props._pos = p.pos;
      props._d = a.d;
      props._justify = a.justify;
      if (p.layout.alt !== undefined && spec.alt) {
        props._head = p.layout.lines.slice(0, -1).map((s) => `${s}\n`).join("");
        props._last = p.layout.lines[p.layout.lines.length - 1]!;
        props._alt = p.layout.alt;
      } else props._text = p.layout.lines.join("\n");
    } else {
      props._anchor = "center";
      props._pos = "center";
      props._d = 0;
      props._justify = "center";
      props._text = "";
    }
    return { type: "Feature" as const, id, geometry: { type: "Point" as const, coordinates: m.lngLat }, properties: props };
  }

  /** Raster pixel classes of the view from the `index` tiles, plus the drawn lines. */
  private async classGrid(W: number, H: number, z: number): Promise<ClassGrid | null> {
    const r = this.opts.raster!;
    const map = this.map;
    const grid = new ClassGrid(W, H, 4);
    const za = Math.max(0, Math.min(r.maxZoom, Math.round(z - 1)));
    const n = 2 ** za;
    const cells: { c: number; r: number; tx: number; ty: number; px: number; py: number }[] = [];
    const need = new Set<string>();
    for (let row = 0; row < grid.rows; row++) {
      for (let col = 0; col < grid.cols; col++) {
        const x = (col + 0.5) * grid.cell;
        const y = (row + 0.5) * grid.cell;
        const ll = map.unproject([x, y]);
        const lat = this.opts.fromMapLat(ll.lat);
        if (!Number.isFinite(lat)) continue;
        const lng = ((((ll.lng + 180) % 360) + 360) % 360) - 180;
        const wx = ((lng + 180) / 360) * n;
        const s = Math.sin((Math.max(-85.05, Math.min(85.05, lat)) * Math.PI) / 180);
        const wy = (0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI)) * n;
        const tx = Math.min(n - 1, Math.floor(wx));
        const ty = Math.min(n - 1, Math.max(0, Math.floor(wy)));
        cells.push({ c: col, r: row, tx, ty, px: (wx - tx) * 256, py: (wy - ty) * 256 });
        need.add(`${za}/${tx}/${ty}`);
      }
    }
    const loaded = new Map<string, { values: Uint8Array; width: number; height: number } | null>();
    await Promise.all([...need].map(async (k) => loaded.set(k, await this.indexTile(k))));
    for (const cell of cells) {
      const t = loaded.get(`${za}/${cell.tx}/${cell.ty}`);
      if (!t) continue;
      const x = Math.min(t.width - 1, Math.floor((cell.px * t.width) / 256));
      const y = Math.min(t.height - 1, Math.floor((cell.py * t.height) / 256));
      grid.setClass(cell.c, cell.r, classOfKind(r.kinds[t.values[y * t.width + x]!] ?? 4));
    }
    this.addLines(grid, z);
    grid.finish();
    return grid;
  }

  private indexTile(k: string) {
    let p = this.tiles.get(k);
    if (!p) {
      const [z, x, y] = k.split("/").map(Number) as [number, number, number];
      p = this.archive!.getZxy(z, x, y)
        .then(async (t) => (t ? await decodePngValues(new Uint8Array(t.data)) : null))
        .catch(() => null);
      this.tiles.set(k, p);
      if (this.tiles.size > 256) this.tiles.delete(this.tiles.keys().next().value!);
    }
    return p;
  }

  /** Drawn lines mark their pixels (line pens are mostly `b14 = 3`, pixel class 6). */
  private addLines(grid: ClassGrid, z: number): void {
    const map = this.map;
    const style = map.getStyle();
    const widthByClass = new Map<string, Map<number, number>>();
    for (const layer of style.layers as AnyLayer[]) {
      if (layer.type !== "line" || !layer.source || !layer["source-layer"]) continue;
      if (layer.layout?.visibility === "none" || z < (layer.minzoom ?? 0) || z >= (layer.maxzoom ?? 24)) continue;
      if (JSON.stringify(layer.paint ?? {}).includes("feature-state")) continue; // hover highlights
      const k = `${layer.source}/${layer["source-layer"]}`;
      const m = widthByClass.get(k) ?? new Map<number, number>();
      widthByClass.set(k, m);
      const w = typeof layer.paint?.["line-width"] === "number" ? (layer.paint["line-width"] as number) : 1;
      const ids: number[] = [];
      const walk = (f: unknown): void => {
        if (!Array.isArray(f)) return;
        const [op, a, b] = f as unknown[];
        const isClass = Array.isArray(a) && a[0] === "get" && a[1] === "class";
        if (op === "==" && isClass && typeof b === "number") ids.push(b);
        else if (op === "in" && isClass && Array.isArray(b) && b[0] === "literal") ids.push(...(b[1] as number[]));
        else f.forEach(walk);
      };
      walk(layer.filter);
      for (const id of ids) m.set(id, Math.max(m.get(id) ?? 0, w));
    }
    for (const [k, widths] of widthByClass) {
      const [source, sourceLayer] = k.split("/") as [string, string];
      if (!map.getSource(source)) continue;
      for (const f of map.querySourceFeatures(source, { sourceLayer }) as unknown as TileFeature[]) {
        const w = widths.get(f.properties.class as number);
        if (!w) continue;
        for (const line of tileLines(f)) {
          let prev: maplibregl.Point | null = null;
          for (const ll of line) {
            const q = map.project(ll);
            if (prev) grid.addSegment(prev.x, prev.y, q.x, q.y, w);
            prev = q;
          }
        }
      }
    }
  }
}
