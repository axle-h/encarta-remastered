/**
 * Where the atlas data lives and how to fetch it.
 *
 * The converter writes root-relative URLs into the styles (`pmtiles:///atlas/vector/earth.pmtiles`,
 * `/atlas/glyphs/{fontstack}/{range}.pbf`, `/atlas/sprites/sprite`) for a server that serves `out/` at
 * `/`. The content server serves it at `/out/atlas/` (docs/api.md), and MapLibre needs absolute
 * sprite and glyph URLs, so every such URL is rewritten here: `/atlas/…` → `<dataUrl>/…`, then made
 * absolute against the page origin. URLs that are already absolute are left alone, so this keeps
 * working once the server rewrites them itself (docs/api.md "Static files"), except same-origin `/atlas/…`.
 */
import type { StyleSpecification } from "maplibre-gl";
import type { StyleIndexEntry } from "./catalog";

export interface Endpoints {
  /** Base URL of `out/atlas/` (default `/out/atlas`). */
  dataUrl: string;
  /** Base URL of the JSON API (default `/api`). */
  apiUrl: string;
  /** Origin that root-relative URLs resolve against (default `location.origin`). */
  origin: string;
}

export function defaultEndpoints(partial: Partial<Endpoints> = {}): Endpoints {
  const origin = partial.origin ?? (typeof location !== "undefined" ? location.origin : "http://localhost");
  return {
    dataUrl: trimSlash(partial.dataUrl ?? "/out/atlas"),
    apiUrl: trimSlash(partial.apiUrl ?? "/api"),
    origin: trimSlash(origin),
  };
}

function trimSlash(s: string): string {
  return s.length > 1 ? s.replace(/\/+$/, "") : s;
}

const ABSOLUTE = /^[a-z][a-z0-9+.-]*:\/\//i;

/** Absolute URL for a path. String concatenation keeps `{z}`/`{fontstack}` template braces intact. */
export function absolute(path: string, ep: Endpoints): string {
  if (ABSOLUTE.test(path)) return path;
  if (path.startsWith("//")) return `${ep.origin.split("//")[0]}${path}`;
  if (path.startsWith("/")) return ep.origin + path;
  return `${ep.origin}/${path}`;
}

/** `/atlas/…` (the converter's root) → `<dataUrl>/…`; anything else root-relative → origin. */
export function resolveDataUrl(url: string, ep: Endpoints): string {
  if (url.startsWith("pmtiles://")) {
    const rest = url.slice("pmtiles://".length);
    return ABSOLUTE.test(rest) ? url : `pmtiles://${resolveDataUrl(rest, ep)}`;
  }
  if (ABSOLUTE.test(url)) {
    // The server rewrites style URLs to `<request origin>/atlas/…`. Behind the Vite dev proxy that origin
    // is Vite's, where `/atlas/…` is a client route, so same-origin atlas URLs go through `dataUrl` too.
    const sameOrigin = url.startsWith(ep.origin + "/atlas/");
    return sameOrigin ? resolveDataUrl(url.slice(ep.origin.length), ep) : url;
  }
  if (url === "/atlas" || url.startsWith("/atlas/")) return absolute(ep.dataUrl + url.slice("/atlas".length), ep);
  return absolute(url, ep);
}

/** URL of a file under `out/atlas/`, e.g. `dataFile("raster/palettes.json")`. */
export function dataFile(rel: string, ep: Endpoints): string {
  return absolute(`${ep.dataUrl}/${rel.replace(/^\/+/, "")}`, ep);
}

/** The PMTiles archive of a raster's index (CMT values) or of one of its LOOKs. */
export function rasterArchive(raster: string, look: string | "index", ep: Endpoints): string {
  return dataFile(`raster/${raster.toLowerCase()}/${look.toLowerCase()}.pmtiles`, ep);
}

/** Rewrites a converted style's URLs for this client (see the file comment). Returns a copy. */
export function resolveStyleUrls(style: StyleSpecification, ep: Endpoints): StyleSpecification {
  const out: StyleSpecification = { ...style, sources: {} };
  if (typeof style.sprite === "string") out.sprite = resolveDataUrl(style.sprite, ep);
  else if (Array.isArray(style.sprite)) out.sprite = style.sprite.map((s) => ({ ...s, url: resolveDataUrl(s.url, ep) }));
  if (style.glyphs) out.glyphs = resolveDataUrl(style.glyphs, ep);
  for (const [id, src] of Object.entries(style.sources)) {
    const s = { ...src } as Record<string, unknown>;
    if (typeof s.url === "string") s.url = resolveDataUrl(s.url, ep);
    if (Array.isArray(s.tiles)) s.tiles = (s.tiles as string[]).map((t) => resolveDataUrl(t, ep));
    (out.sources as Record<string, unknown>)[id] = s;
  }
  return out;
}

/** MapLibre `transformRequest`: a backstop for any root-relative URL the style transform missed. */
export function makeTransformRequest(ep: Endpoints): (url: string) => { url: string } {
  return (url: string) => ({ url: url.startsWith("/") ? resolveDataUrl(url, ep) : url });
}

// ---------------------------------------------------------------------------------------------
// Fetching

export class AtlasApiError extends Error {
  constructor(
    readonly status: number,
    readonly url: string,
    message: string,
  ) {
    super(message);
  }
}

const jsonCache = new Map<string, Promise<unknown>>();

export async function fetchJson<T>(url: string, signal?: AbortSignal, cache = true): Promise<T> {
  if (cache && jsonCache.has(url)) return jsonCache.get(url) as Promise<T>;
  const p = (async () => {
    const r = await fetch(url, { signal, headers: { accept: "application/json" } });
    if (!r.ok) {
      let msg = `${r.status} ${r.statusText}`;
      try {
        const body = (await r.json()) as { error?: string };
        if (body?.error) msg = body.error;
      } catch {
        /* not JSON */
      }
      throw new AtlasApiError(r.status, url, msg);
    }
    return (await r.json()) as T;
  })();
  if (cache) {
    jsonCache.set(url, p);
    p.catch(() => jsonCache.delete(url));
  }
  return p;
}

export function loadStyleIndex(ep: Endpoints, signal?: AbortSignal): Promise<StyleIndexEntry[]> {
  return fetchJson<StyleIndexEntry[]>(dataFile("styles/index.json", ep), signal);
}

export function loadStyle(entry: StyleIndexEntry, ep: Endpoints, signal?: AbortSignal): Promise<StyleSpecification> {
  return fetchJson<StyleSpecification>(dataFile(entry.file, ep), signal);
}

/** `raster/palettes.json`: every LOOK of every raster (colour per CMT value, pixel class, RCT legend). */
export interface Look {
  colours: string[];
  shade?: number[];
  pixel_class?: number[];
  legend?: string[];
}
export interface RasterPalette {
  looks: Record<string, Look>;
  max_level: number;
  min_altitude_km: number;
}
export type Palettes = Record<string, RasterPalette>;

export function loadPalettes(ep: Endpoints, signal?: AbortSignal): Promise<Palettes> {
  return fetchJson<Palettes>(dataFile("raster/palettes.json", ep), signal);
}

// ---------------------------------------------------------------------------------------------
// Gazetteer and place links (docs/api.md; `/api/atlas/*` is this client's request to the server)

export interface PlaceHit {
  entity: number;
  name: string;
  feature_type?: string | null;
  context?: string | null;
  class_name?: string | null;
  lat: number;
  lon: number;
  goto_km?: number | null;
  west?: number | null;
  south?: number | null;
  east?: number | null;
  north?: number | null;
  body?: "earth" | "moon";
  link_count?: number;
}

export interface PlaceLink {
  ordinal?: number;
  kind: number;
  kind_label: string;
  ref_id: number;
  title?: string | null;
  item_kind?: string | null;
}

export function searchPlaces(
  q: string,
  ep: Endpoints,
  opts: { body?: "earth" | "moon"; limit?: number; signal?: AbortSignal } = {},
): Promise<PlaceHit[]> {
  const p = new URLSearchParams({ q });
  if (opts.body) p.set("body", opts.body);
  if (opts.limit) p.set("limit", String(opts.limit));
  const url = absolute(`${ep.apiUrl}/atlas/search?${p}`, ep);
  return fetchJson<{ places?: PlaceHit[]; hits?: PlaceHit[] }>(url, opts.signal).then((r) => r.places ?? r.hits ?? []);
}

export interface PlaceDetail {
  place: PlaceHit & { abbr?: string | null; importance?: number; classes?: number[] | string };
  names?: { id: number; entity: number; name: string; kind: string }[];
  links?: PlaceLink[];
}

/** `GET /api/atlas/place/{entity}`: the gazetteer row, its names and its links. */
export function getPlace(entity: number, ep: Endpoints, signal?: AbortSignal): Promise<PlaceDetail> {
  return fetchJson<PlaceDetail>(absolute(`${ep.apiUrl}/atlas/place/${entity}`, ep), signal);
}

/** P2MTBL links of a place (`GET /api/place/{gid}`); a place without links is `[]`, not an error. */
export async function getPlaceLinks(entity: number, ep: Endpoints, signal?: AbortSignal): Promise<PlaceLink[]> {
  try {
    const r = await fetchJson<{ links: PlaceLink[] }>(absolute(`${ep.apiUrl}/place/${entity}`, ep), signal);
    return r.links ?? [];
  } catch (e) {
    if (e instanceof AtlasApiError && e.status === 404) return [];
    throw e;
  }
}

/** The link "Read Article" opens: the first `article` link, else the first link that is not media. */
export function primaryArticle(links: readonly PlaceLink[]): PlaceLink | undefined {
  return links.find((l) => l.kind_label === "article") ?? links.find((l) => l.item_kind === "article");
}

const KIND_LABELS: Record<string, string> = {
  article: "Article",
  "flag-and-anthem": "Flag and anthem",
  "facts-and-figures": "Facts and figures",
  "flag-media": "Flag",
  "flag-companion-media": "Flag",
  music: "Music",
};

/** Menu text of a link: its item title, with the kind when the title alone is ambiguous. */
export function linkLabel(l: PlaceLink): string {
  const kind = KIND_LABELS[l.kind_label] ?? (l.kind_label.startsWith("photo:") ? `Photo (${l.kind_label.slice(6)})` : "Media");
  if (!l.title) return kind;
  return l.kind_label === "article" ? l.title : `${l.title} — ${kind}`;
}

// ---------------------------------------------------------------------------------------------
// Statistics (MSWORLD.SDB, docs/api.md "Statistics")

export interface StatIndicator {
  id: number;
  name: string;
  categories?: string[];
  unit?: { unit?: string | null; alt_unit?: string | null } | null;
  source?: string | null;
  content_ref_id?: number | null;
}
export interface StatsIndex {
  categories: { bit: number; name: string }[];
  indicators: StatIndicator[];
  countries: { id: number; name: string }[];
}
export interface StatValue {
  country_id: number;
  country: string;
  rank?: number | null;
  value?: number | null;
  value_text?: string | null;
  year?: number | string | null;
}

export function loadStats(ep: Endpoints, signal?: AbortSignal): Promise<StatsIndex> {
  return fetchJson<StatsIndex>(absolute(`${ep.apiUrl}/stats`, ep), signal);
}

export function loadIndicator(id: number, ep: Endpoints, signal?: AbortSignal): Promise<{ indicator: StatIndicator; values: StatValue[] }> {
  return fetchJson(absolute(`${ep.apiUrl}/stats/indicator/${id}`, ep), signal);
}
