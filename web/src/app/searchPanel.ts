import type { QuickKind } from "./featuresData";

export const QUICK_SEARCH_EVENT = "encarta:quicksearch";

/** Asks the shell to show the Quick Search dialog for one category. */
export function openQuickSearch(kind: QuickKind) {
  window.dispatchEvent(new CustomEvent<QuickKind>(QUICK_SEARCH_EVENT, { detail: kind }));
}

/** Result categories of the panel: the label, the API `kind` filter and the "Search all" noun. */
export const PANEL_CATEGORIES = [
  { id: "article", label: "Articles", kind: "article", all: "articles", icon: "article" },
  { id: "media", label: "Photos & More", kind: "media", all: "photos & more", icon: "image" },
  { id: "web", label: "Web Links", kind: "web", all: "Web links", icon: "arrow" },
  { id: "dictionary", label: "Dictionaries", kind: "dictionary", all: "dictionaries", icon: "book" },
  { id: "map", label: "Maps", kind: "map", all: "maps", icon: "map" },
  { id: "quotation", label: "Quotations", kind: "quotation", all: "quotations", icon: "text" },
] as const;

export type PanelCategory = (typeof PANEL_CATEGORIES)[number];

export const QUICK_LABEL: Record<QuickKind, string> = {
  article: "Articles",
  map: "Maps",
  media: "Photos & More",
  quotation: "Quotations",
  dictionary: "Dictionary",
};
