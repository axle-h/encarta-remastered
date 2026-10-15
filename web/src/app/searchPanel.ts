import type { QuickKind } from "./featuresData";

export const QUICK_SEARCH_EVENT = "encarta:quicksearch";

/** Asks the shell to show the Quick Search dialog for one category. */
export function openQuickSearch(kind: QuickKind) {
  window.dispatchEvent(new CustomEvent<QuickKind>(QUICK_SEARCH_EVENT, { detail: kind }));
}

/**
 * Result categories of the panel: the label, the API `kind` filter and the "Search all" noun.
 * Quotations is only a Quick Search category: the VM's panel never lists it (quotation hits show
 * up in Best Matches).
 */
export const PANEL_CATEGORIES = [
  { id: "article", label: "Articles", kind: "article", all: "articles", icon: "article" },
  { id: "media", label: "Photos & More", kind: "media", all: "photos & more", icon: "image" },
  { id: "web", label: "Web Links", kind: "web", all: "Web links", icon: "arrow" },
  { id: "dictionary", label: "Dictionaries", kind: "dictionary", all: "dictionaries", icon: "book" },
  { id: "map", label: "Maps", kind: "map", all: "maps", icon: "map" },
  { id: "quotation", label: "Quotations", kind: "quotation", all: "quotations", icon: "text", quickOnly: true },
] as const;

export type PanelCategory = (typeof PANEL_CATEGORIES)[number];

/** The categories the panel lists: all but the Quick-Search-only ones, or just the one asked for. */
export function panelCategories(only?: string): PanelCategory[] {
  return PANEL_CATEGORIES.filter((c) => (only ? c.id === only : !("quickOnly" in c)));
}

/** Panel width limits measured on the VM's splitter (px, border included). */
export const PANEL_WIDTH = { min: 308, initial: 378, max: 498 } as const;

export function clampPanelWidth(w: number): number {
  return Math.round(Math.min(PANEL_WIDTH.max, Math.max(PANEL_WIDTH.min, w)));
}

export const QUICK_LABEL: Record<QuickKind, string> = {
  article: "Articles",
  map: "Maps",
  media: "Photos & More",
  quotation: "Quotations",
  dictionary: "Dictionary",
};
