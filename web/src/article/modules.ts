/**
 * The article's right column ("modules"), as the reader builds it (content-schema §11, measured on
 * the 25 encyclopedia captures in the VM). Pure functions, so they can be unit-tested.
 *
 * Order, when present: World Atlas, Quick Facts, Related Articles, Quotations, Web Sites, Primary
 * Sources, Sidebars, one Virtual Tour / 3D Ancient Site per tour, Further Reading, Archives (then the
 * reader's Visual Browser and the online-only Radio and News modules, which we do not draw).
 */
import type { AssocItem } from "../api/types";
import type { Inline } from "../content/model";

/** Sidebar categories (`sidebar_category`, from DATA `type` SBD SBH SBI SBL SBO) listed under "Primary Sources". */
const PRIMARY_SOURCE_CATEGORIES = new Set([
  "HISTORIC DOCUMENTS",
  "HISTORIC HEADLINES",
  "INTERVIEWS",
  "GREAT WORKS OF LITERATURE",
  "HISTORIC SPEECHES",
]);

export function isPrimarySource(a: AssocItem): boolean {
  return PRIMARY_SOURCE_CATEGORIES.has(a.sidebar_category ?? "");
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

/**
 * Sort key of a title the way the reader sorts module lists: by the title's text as stored, case
 * ignored, apostrophes and hyphens ignored (Windows "word sort"), punctuation before digits before
 * letters. A title that starts with markup (`<it>God Save the Queen</it>`) is stored pretty-printed
 * with leading white space, so it sorts first (VM: United Kingdom's Primary Sources).
 */
export function titleSortKey(a: Pick<AssocItem, "title" | "title_html">): string {
  const html = a.title_html ?? a.title ?? "";
  const lead = html.trimStart().startsWith("<") ? " " : "";
  const text = html
    .replace(/<[^>]*>/g, "")
    .replace(/&(#x[0-9a-f]+|#\d+|\w+);/gi, (m, e: string) =>
      e[0] === "#" ? String.fromCodePoint(e[1] === "x" || e[1] === "X" ? parseInt(e.slice(2), 16) : Number(e.slice(1))) : (ENTITIES[e.toLowerCase()] ?? m),
    );
  let key = "";
  for (const ch of (lead + text).toLowerCase().replace(/['’‘-]/g, "")) {
    // Three classes: everything else < digits < letters; within a class by code point.
    const cls = /\p{L}/u.test(ch) ? "2" : /\p{N}/u.test(ch) ? "1" : "0";
    key += cls + ch;
  }
  return key;
}

function sortBy<T>(items: T[], key: (t: T) => string): T[] {
  const keyed = items.map((it) => [key(it), it] as const);
  keyed.sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));
  return keyed.map((k) => k[1]);
}

export function byTitle<T extends Pick<AssocItem, "title" | "title_html">>(items: T[]): T[] {
  return sortBy(items, titleSortKey);
}

function unique(items: AssocItem[]): AssocItem[] {
  const seen = new Set<number>();
  return items.filter((m) => !seen.has(m.ref_id) && seen.add(m.ref_id));
}

export interface ModuleLists {
  /** First `maps` association: the World Atlas module. */
  map?: AssocItem;
  quotations: AssocItem[];
  webSites: AssocItem[];
  primarySources: AssocItem[];
  sidebars: AssocItem[];
  /** Virtual tours (`virttour`), in DATA order: "Virtual Tour" (2DVT) or "3D Ancient Site" (3DVT). */
  tours: AssocItem[];
  furtherReading: AssocItem[];
  archives: AssocItem[];
}

/**
 * The lists behind the modules, from the article's forward associations (DATA order). Web Sites,
 * Primary Sources and Sidebars are sorted by title, Archives by year then title, Further Reading,
 * Quotations and tours keep the DATA order (all checked against the VM captures).
 */
export function moduleLists(media: AssocItem[]): ModuleLists {
  const of = (g: string) => unique(media.filter((m) => m.group === g));
  const sidebars = of("sidebar");
  const archives = of("archive");
  const archiveKey = (a: AssocItem) => `${String(Number(a.year ?? 0)).padStart(5, "0")}${titleSortKey(a)}`;
  return {
    map: media.find((m) => m.group === "maps"),
    quotations: of("quotation"),
    webSites: byTitle(of("weblink")),
    primarySources: byTitle(sidebars.filter(isPrimarySource)),
    sidebars: byTitle(sidebars.filter((s) => !isPrimarySource(s))),
    tours: unique(media.filter((m) => m.group === "virttour" || m.group === "vtour")),
    furtherReading: of("furtherread"),
    archives: sortBy(archives, archiveKey),
  };
}

/** Rows a list module shows before "Show All": three, Archives and Further Reading five (VM). */
export const MODULE_ROWS = { list: 3, archives: 5, furtherReading: 5, related: 5 } as const;

/** The label of an archive (yearbook) row: "1938: France". */
export function archiveLabel(a: AssocItem): string {
  const t = a.title ?? `Item ${a.ref_id}`;
  return a.year ? `${a.year}: ${t}` : t;
}

/** The module title of a virtual tour. */
export function tourModuleTitle(a: AssocItem): string {
  return a.subtype === "3DVT" ? "3D Ancient Site" : "Virtual Tour";
}

interface FactEntry {
  name?: Inline[];
  value?: Inline[];
  entries?: FactEntry[];
}

/** One line pair of the Quick Facts box: a name (bold) and/or a value. */
export interface QuickFactRow {
  name?: Inline[];
  value?: Inline[];
}

/** A Quick Facts entry: its rows (the entry's own name and value, then its continuations). */
export type QuickFactGroup = QuickFactRow[];

const has = (x: Inline[] | undefined): x is Inline[] => !!x && x.length > 0;

/**
 * The Quick Facts box (VM, 13 captures): the table's first five entries with their continuation
 * values or sub-entries, the fifth entry cut after its first value (for an entry without a value of
 * its own, its first sub-entry). A biography's flat `quickfacts` list counts a named entry plus the
 * unnamed entries after it as one entry; a Facts and Figures table counts the entries of its
 * sections. The `quick` marks of Facts and Figures entries are not used.
 */
export function quickFactGroups(json: unknown, limit = 5): QuickFactGroup[] {
  const j = json as { entries?: FactEntry[]; sections?: { entries?: FactEntry[] }[] } | undefined;
  const groups: QuickFactGroup[] = [];
  const rowsOf = (e: FactEntry): QuickFactRow[] => {
    const out: QuickFactRow[] = [{ name: has(e.name) ? e.name : undefined, value: has(e.value) ? e.value : undefined }];
    for (const sub of e.entries ?? []) out.push({ name: has(sub.name) ? sub.name : undefined, value: has(sub.value) ? sub.value : undefined });
    return out;
  };
  if (j?.entries) {
    for (const e of j.entries) {
      if (has(e.name) || groups.length === 0) groups.push(rowsOf(e));
      else groups[groups.length - 1]!.push(...rowsOf(e));
    }
  } else {
    for (const sec of j?.sections ?? []) for (const e of sec.entries ?? []) groups.push(rowsOf(e));
  }
  const shown = groups.slice(0, limit);
  if (shown.length === limit) {
    const last = shown[limit - 1]!;
    shown[limit - 1] = last[0]?.value ? [last[0]] : last.slice(0, 2);
  }
  return shown;
}
