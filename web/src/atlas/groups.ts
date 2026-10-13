/**
 * The Map Customizer's feature groups (V4 "Map Customizer" shot: capitals, populated places by size,
 * borders, transportation, physical features …), built from the classes the style draws. Group
 * membership is by PDCAttribute class name; Encarta's own grouping lives in ENCARTA.EXE and is not
 * converted.
 */
import type { StyleSpecification } from "maplibre-gl";
import { HIT_PREFIX, STAT_LAYER, isHoverLayer, layerClasses } from "./layers";
import { prettyClassName } from "./legend";

export interface ClassItem {
  id: number;
  name: string;
  /** Shown by default somewhere in the zoom range (else only as an option). */
  byDefault: boolean;
}

export interface ClassGroup {
  key: string;
  label: string;
  classes: ClassItem[];
}

const GROUPS: { key: string; label: string; test: RegExp }[] = [
  { key: "places", label: "Populated places", test: /^PPL|capital|city|town|village|Urban|populated/i },
  { key: "borders", label: "Borders", test: /boundary|border|treaty|disputed|cease-?fire|demarcation/i },
  { key: "transport", label: "Transportation", test: /Road|Rail|Ferry|Airport|Tunnel|Highway|Street|Shield|Route|Port|Bridge|Trail/i },
  { key: "water", label: "Water features", test: /River|Lake|Sea\b|Ocean|Bay|Gulf|Strait|Canal|Reservoir|Water|Glacier|Ice|Bathymetric|Sea floor|trench|Reef|Fall/i },
  { key: "physical", label: "Physical features", test: /Mountain|Peak|Volcano|Desert|Plain|Plateau|Island|Physiographic|Cape|Valley|Basin|Hypsometric|Wetland|Beach|Pass|Landmark|Crater|MOON/i },
  { key: "parks", label: "Parks and reserves", test: /Park|Reserve|Forest|Monument|Heritage/i },
  { key: "grid", label: "Grid lines", test: /Graticule|Equator|Tropic|Circle|Meridian|Date line|Latitude|Longitude/i },
  { key: "regions", label: "Countries and regions", test: /Sovereign|Admin|Region|Continent|Country|Dependency|Province|State/i },
  { key: "themes", label: "Map theme", test: /^(LANG|REL|ECO|CLIM|TECT|Time zone|Daylight|Z - )/ },
];

export function groupOf(className: string): string {
  return GROUPS.find((g) => g.test.test(className))?.key ?? "other";
}

export function classGroups(raw: StyleSpecification): ClassGroup[] {
  const items = new Map<number, ClassItem>();
  for (const layer of raw.layers) {
    if (layer.id.startsWith(HIT_PREFIX) || layer.id === STAT_LAYER || isHoverLayer(layer)) continue;
    const { ids, names } = layerClasses(layer);
    const optional = (layer as { layout?: { visibility?: string } }).layout?.visibility === "none";
    ids.forEach((id, i) => {
      const raw = names[i] ?? `class ${id}`;
      if (raw.startsWith("Z - (STAT)") || /^\{.*\}$/.test(raw)) return;
      const prev = items.get(id);
      if (prev) prev.byDefault ||= !optional;
      else items.set(id, { id, name: raw, byDefault: !optional });
    });
  }
  const byGroup = new Map<string, ClassItem[]>();
  for (const item of items.values()) {
    const k = groupOf(item.name);
    if (!byGroup.has(k)) byGroup.set(k, []);
    byGroup.get(k)!.push({ ...item, name: prettyClassName(item.name) });
  }
  const all = [...GROUPS, { key: "other", label: "Other features" }];
  return all
    .filter((g) => byGroup.has(g.key))
    .map((g) => ({ key: g.key, label: g.label, classes: byGroup.get(g.key)!.sort((a, b) => a.name.localeCompare(b.name)) }));
}

/** Is the class shown, given the user's overrides? */
export function classShown(item: ClassItem, on: ReadonlySet<number>, off: ReadonlySet<number>): boolean {
  if (off.has(item.id)) return false;
  return item.byDefault || on.has(item.id);
}

/** Toggle a set of classes to `show`. Returns new override sets. */
export function setShown(items: readonly ClassItem[], show: boolean, on: ReadonlySet<number>, off: ReadonlySet<number>): { on: Set<number>; off: Set<number> } {
  const nOn = new Set(on);
  const nOff = new Set(off);
  for (const it of items) {
    if (show) {
      nOff.delete(it.id);
      if (!it.byDefault) nOn.add(it.id);
    } else {
      nOn.delete(it.id);
      nOff.add(it.id);
    }
  }
  return { on: nOn, off: nOff };
}
