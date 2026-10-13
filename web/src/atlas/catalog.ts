/**
 * The map styles: the converter's `styles/index.json` (one entry per MapStyle record) and the names
 * Encarta's style carousel shows for them. Menu names and the `style=` numbers of `msencarta://` URLs
 * were read off the reference-VM shots (fixtures V4: one shot per URL style number), not from the disc.
 */

export interface StyleIndexEntry {
  id: number;
  name: string;
  file: string;
  raster: string;
  look: string;
  default?: boolean;
}

export interface MenuStyle {
  /** MapStyle id (PDCAltitudeGrid key). */
  id: number;
  /** The carousel caption in Encarta 2009. */
  label: string;
  body: "earth" | "moon";
}

/** Encarta's carousel, in its order (V4 panel shots). The Moon is its own world in Encarta. */
export const MENU_STYLES: readonly MenuStyle[] = [
  { id: 0, label: "Comprehensive", body: "earth" },
  { id: 10, label: "Political", body: "earth" },
  { id: 9, label: "Physical Features", body: "earth" },
  { id: 12, label: "Statistical", body: "earth" },
  { id: 7, label: "Earth By Day", body: "earth" },
  { id: 8, label: "Earth At Night", body: "earth" },
  { id: 2, label: "Tectonic", body: "earth" },
  { id: 4, label: "Ecoregions", body: "earth" },
  { id: 3, label: "Climate", body: "earth" },
  { id: 1, label: "Temperature (January)", body: "earth" },
  { id: 25, label: "Temperature (July)", body: "earth" },
  { id: 23, label: "Precipitation (January)", body: "earth" },
  { id: 24, label: "Precipitation (July)", body: "earth" },
  { id: 22, label: "Precipitation (Annual)", body: "earth" },
  { id: 6, label: "Population Density", body: "earth" },
  { id: 21, label: "Languages", body: "earth" },
  { id: 26, label: "Religions", body: "earth" },
  { id: 5, label: "Time Zones", body: "earth" },
  { id: 13, label: "Outline", body: "earth" },
  { id: 16, label: "The Moon", body: "moon" },
];

/**
 * `msencarta://…/style=<n>` → MapStyle id, from the V4 shots. Numbers that showed no distinct look
 * (18, 21–23, 26–33) are left out; callers fall back to the default style.
 */
export const MENU_STYLE: Readonly<Record<number, number>> = {
  0: 0, 1: 10, 2: 2, 3: 9, 4: 12, 5: 7, 6: 8, 7: 4, 8: 1, 9: 25, 10: 23, 11: 24, 12: 6, 13: 5, 14: 12,
  15: 3, 16: 22, 17: 13, 19: 11, 20: 16, 24: 21, 25: 26,
};

export const MOON_STYLE_ID = 16;

/** File name without directory and `.json`: `styles/atlas-political.json` → `atlas-political`. */
export function styleSlug(e: Pick<StyleIndexEntry, "file">): string {
  return e.file.replace(/^.*\//, "").replace(/\.json$/i, "");
}

export function styleLabel(e: StyleIndexEntry): string {
  return MENU_STYLES.find((m) => m.id === e.id)?.label ?? e.name;
}

export function styleBody(e: Pick<StyleIndexEntry, "raster">): "earth" | "moon" {
  return e.raster.toUpperCase() === "MOON" ? "moon" : "earth";
}

/**
 * Find a style by MapStyle id, slug (`atlas-political`), MapStyle name or carousel caption
 * (case-insensitive). Unknown → the default style.
 */
export function findStyle(index: readonly StyleIndexEntry[], key: string | number | undefined | null): StyleIndexEntry | undefined {
  const fallback = index.find((e) => e.default) ?? index[0];
  if (key === undefined || key === null || key === "") return fallback;
  if (typeof key === "number" || /^\d+$/.test(key)) {
    const id = Number(key);
    return index.find((e) => e.id === id) ?? fallback;
  }
  const k = key.toLowerCase();
  return (
    index.find((e) => styleSlug(e).toLowerCase() === k) ??
    index.find((e) => e.name.toLowerCase() === k) ??
    index.find((e) => styleLabel(e).toLowerCase() === k) ??
    fallback
  );
}

/** Carousel entries first (in Encarta's order), then every other style under its MapStyle name. */
export function orderedStyles(index: readonly StyleIndexEntry[]): { menu: StyleIndexEntry[]; more: StyleIndexEntry[] } {
  const byId = new Map(index.map((e) => [e.id, e]));
  const menu = MENU_STYLES.map((m) => byId.get(m.id)).filter((e): e is StyleIndexEntry => !!e);
  const inMenu = new Set(menu.map((e) => e.id));
  const more = index.filter((e) => !inMenu.has(e.id)).sort((a, b) => a.id - b.id);
  return { menu, more };
}
