/**
 * Encarta's home screens are DATA items (`group="home"`, content-schema §3): a home page's forward
 * associations are its links (`home` sub-pages, `article`s, `feat` features), its theme pieces
 * (`hthm`: separators, logo, panel, decoration) and its background (`hsbg`), in `sequence` order.
 */
import type { AssocItem } from "../api/types";
import type { Item } from "../api/types";

/**
 * The Premium home screen (subgroup "Home 3col" with the Dictionary and Encarta Kids features).
 * 701712540 and 701750687 are the Standard edition's variants.
 */
export const HOME_ID = 701712539;
/**
 * Home › Games & Fun Stuff. In the VM it is the only way to the games (folders Animals & Science,
 * Arts & Culture, Geography, History, Tours, Videos; Geography Quiz sits under Geography); no menu
 * lists them. Our extra game pages (`/games`) are linked from the bottom of this page.
 */
export const GAMES_HOME_ID = 701712546;
/** The Encarta Kids home screen. */
export const KIDS_HOME_ID = 701639831;

export function bySequence(a: AssocItem, b: AssocItem): number {
  return (a.sequence ?? 0) - (b.sequence ?? 0);
}

export interface HomeSection {
  title?: string;
  caption?: string;
  separator?: AssocItem;
  links: AssocItem[];
}

const LINK_GROUPS = new Set(["home", "article", "feat", "kids", "maps", "media", "video", "virttour", "sidebar", "table", "audio", "audionested", "flash", "composite", "quotation", "timetopic", "timeevent", "archive", "aart", "litguide"]);

/** A home page's links in `sequence` order, without its theme pieces and background. */
export function homeLinks(item: Item): AssocItem[] {
  return [...(item.media ?? [])].filter((m) => LINK_GROUPS.has(m.group)).sort(bySequence);
}

/** Splits a home page's links at its separators (`hthm` items with a Separator subgroup). */
export function homeSections(item: Item, separators: Map<number, Item> = new Map()): HomeSection[] {
  const media = [...(item.media ?? [])].sort(bySequence);
  const out: HomeSection[] = [];
  let cur: HomeSection = { links: [] };
  for (const m of media) {
    if (m.group === "hthm") {
      const sep = separators.get(m.ref_id);
      const isIcon = sep?.subgroup === "icon" || (m.title ?? "").startsWith("_");
      if (isIcon) continue;
      if (cur.links.length > 0 || cur.title) out.push(cur);
      const title = (m.title ?? "").replace(/ /g, " ").trim();
      const cap = sep?.meta?.caption;
      cur = {
        title: title || undefined,
        caption: Array.isArray(cap) ? cap.filter((x) => typeof x === "string").join("") : undefined,
        separator: m,
        links: [],
      };
    } else if (LINK_GROUPS.has(m.group)) {
      cur.links.push(m);
    }
  }
  if (cur.links.length > 0 || cur.title) out.push(cur);
  return out;
}

/** Theme pieces by role name (`_logo_`, `_decoration_`, `_panel_` …). */
export function themeIcons(item: Item): Map<string, AssocItem> {
  const out = new Map<string, AssocItem>();
  for (const m of item.media ?? []) {
    const t = m.title ?? "";
    if (m.group === "hthm" && /^_.*_$/.test(t)) out.set(t.slice(1, -1), m);
  }
  return out;
}

export function background(item: Item): AssocItem | undefined {
  return [...(item.media ?? [])].sort(bySequence).find((m) => m.group === "hsbg");
}

/** Where a home link goes. */
export function homeLinkHref(m: AssocItem, kids = false): string {
  const pre = kids ? "/kids" : "";
  if (m.group === "home" || m.kind === "home-page") return `${pre}/browse/${m.ref_id}`;
  if (m.group === "feat" || m.kind === "feature") {
    const t = (m.title ?? "").toLowerCase();
    if (t.includes("kids")) return "/kids";
    if (t.includes("dictionar")) return "/dictionary";
    if (t.includes("timeline")) return "/timeline";
    if (t.includes("atlas")) return "/atlas";
  }
  return `${pre}/item/${m.ref_id}`;
}
