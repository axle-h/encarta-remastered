/**
 * The map key: every class the current style draws at the current zoom, with the symbol, line pens or
 * fill it is drawn with, taken from the style's own layers (so it always matches the map). Encarta's
 * "Map Legend" captions come from ENCARTA.EXE resources, which are not converted; the PDCAttribute
 * class names stand in for them. Raster LOOKs add their RCT colour list (palettes.json `legend`).
 */
import type { LayerSpecification, StyleSpecification } from "maplibre-gl";
import { HIT_PREFIX, STAT_LAYER, inZoom, isHoverLayer, layerClasses, layerState, type ClassOverrides } from "./layers";

export interface LegendLine {
  color: string;
  width: number;
  dash?: number[];
  offset?: number;
}

export interface LegendEntry {
  name: string;
  classes: number[];
  kind: "point" | "line" | "area" | "label";
  icon?: string;
  circle?: { color: string; radius: number };
  fill?: { color?: string; pattern?: string; opacity?: number };
  lines: LegendLine[];
  text?: { color: string; font?: string; size?: number; halo?: string };
}

const PREFIXES = /^(LANG|REL|ECO|CLIM|TECT|MOON|TZ|POP|PARK) - /;

export function prettyClassName(name: string): string {
  return name.replace(/^Z - \(STAT\) /, "").replace(PREFIXES, "").trim();
}

function num(v: unknown, d: number): number {
  return typeof v === "number" ? v : d;
}

function str(v: unknown): string | undefined {
  return typeof v === "string" ? v : undefined;
}

export function legendEntries(style: StyleSpecification, zoom: number, o: ClassOverrides, opts: { stats?: boolean } = {}): LegendEntry[] {
  const byName = new Map<string, LegendEntry>();
  for (const layer of style.layers) {
    if (layer.type === "background" || layer.type === "raster") continue;
    if (layer.id.startsWith(HIT_PREFIX) || layer.id === STAT_LAYER || isHoverLayer(layer)) continue;
    if (!inZoom(layer, zoom) || !layerState(layer, o).visible) continue;
    const { ids, names } = layerClasses(layer);
    names.forEach((rawName, i) => {
      const id = ids[i];
      if (id === undefined || o.off.has(id)) return;
      if (!opts.stats && rawName.startsWith("Z - (STAT)")) return;
      const name = prettyClassName(rawName);
      if (!name || /^\{.*\}$/.test(name)) return;
      let e = byName.get(name);
      if (!e) {
        e = { name, classes: [], kind: "label", lines: [] };
        byName.set(name, e);
      }
      if (!e.classes.includes(id)) e.classes.push(id);
      addLayer(e, layer);
    });
  }
  const rank = { point: 0, line: 1, area: 2, label: 3 } as const;
  for (const e of byName.values()) {
    e.kind = e.icon || e.circle ? "point" : e.lines.length ? "line" : e.fill ? "area" : "label";
  }
  return [...byName.values()].sort((a, b) => rank[a.kind] - rank[b.kind]);
}

function addLayer(e: LegendEntry, layer: LayerSpecification): void {
  const paint = (layer.paint ?? {}) as Record<string, unknown>;
  const layout = ((layer as { layout?: Record<string, unknown> }).layout ?? {}) as Record<string, unknown>;
  switch (layer.type) {
    case "fill":
      if (!e.fill) e.fill = { color: str(paint["fill-color"]), pattern: str(paint["fill-pattern"]), opacity: typeof paint["fill-opacity"] === "number" ? paint["fill-opacity"] : undefined };
      break;
    case "line": {
      const color = str(paint["line-color"]);
      if (!color) break;
      const dash = Array.isArray(paint["line-dasharray"]) ? (paint["line-dasharray"] as number[]) : undefined;
      const line: LegendLine = { color, width: num(paint["line-width"], 1), dash, offset: typeof paint["line-offset"] === "number" ? paint["line-offset"] : undefined };
      if (e.lines.length < 6 && !e.lines.some((l) => JSON.stringify(l) === JSON.stringify(line))) e.lines.push(line);
      break;
    }
    case "circle":
      e.circle ??= { color: str(paint["circle-color"]) ?? "#000", radius: num(paint["circle-radius"], 3) };
      break;
    case "symbol": {
      const icon = str(layout["icon-image"]);
      if (icon && !e.icon) e.icon = icon;
      if (layout["text-field"] !== undefined && !e.text) {
        const fonts = layout["text-font"];
        e.text = {
          color: str(paint["text-color"]) ?? "#000",
          font: Array.isArray(fonts) ? String(fonts[0]) : undefined,
          size: typeof layout["text-size"] === "number" ? layout["text-size"] : undefined,
          halo: str(paint["text-halo-color"]),
        };
      }
      break;
    }
    default:
      break;
  }
}

/** CSS font for a glyph face name such as "MS Reference Sans Serif Bold Italic". */
export function cssFont(face: string | undefined, size = 11): string {
  if (!face) return `${size}px sans-serif`;
  const bold = /\bBold\b/.test(face);
  const italic = /\bItalic\b/.test(face);
  const family = face.replace(/\s+(Regular|Bold|Italic)\b/g, "").trim();
  const generic = /Serif(?! Sans)|Times/.test(family) && !/Sans/.test(family) ? "serif" : "sans-serif";
  return `${italic ? "italic " : ""}${bold ? "bold " : ""}${size}px "${family}", ${generic}`;
}
