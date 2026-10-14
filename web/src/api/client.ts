/**
 * Thin JSON client for the content server (docs/api.md), with an in-memory cache so that going
 * back and forth between pages does not refetch. Every request takes an optional AbortSignal.
 */
import type {
  Article,
  DictEntry,
  DictLookupResponse,
  DictionaryInfo,
  Edition,
  Interactive,
  Item,
  PlaceResponse,
  SearchResponse,
  SpellResponse,
  SuggestResponse,
  TimelineMeta,
  TimelineResponse,
  ToursIndex,
} from "./types";

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly url: string,
  ) {
    super(message);
  }
}

type Params = Record<string, string | number | boolean | undefined | null>;

export function query(params: Params): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "" || v === false) continue;
    q.set(k, v === true ? "1" : String(v));
  }
  const s = q.toString();
  return s ? `?${s}` : "";
}

const cache = new Map<string, Promise<unknown>>();
const MAX_CACHE = 400;

export async function getJson<T>(url: string, signal?: AbortSignal, useCache = true): Promise<T> {
  if (useCache) {
    const hit = cache.get(url);
    if (hit) return (await hit) as T;
  }
  const p = (async () => {
    // The shared cache entry must not be cancelled by one caller's signal.
    const res = await fetch(url, { headers: { Accept: "application/json" }, signal: useCache ? undefined : signal });
    if (!res.ok) {
      let msg = `HTTP ${res.status}`;
      try {
        const body = (await res.json()) as { error?: string };
        if (body.error) msg = body.error;
      } catch {
        /* not JSON */
      }
      throw new ApiError(res.status, msg, url);
    }
    return (await res.json()) as T;
  })();
  if (useCache) {
    cache.set(url, p);
    p.catch(() => cache.delete(url));
    if (cache.size > MAX_CACHE) {
      const first = cache.keys().next().value;
      if (first !== undefined) cache.delete(first);
    }
  }
  if (signal) {
    return await new Promise<T>((resolve, reject) => {
      if (signal.aborted) return reject(new DOMException("aborted", "AbortError"));
      signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")), { once: true });
      p.then(resolve as (v: unknown) => void, reject);
    });
  }
  return (await p) as T;
}

const ed = (edition?: Edition) => (edition === "kids" ? "kids" : undefined);

export const api = {
  item: (id: number | string, s?: AbortSignal) => getJson<Item>(`/api/item/${id}`, s),
  article: (id: number | string, edition?: Edition, s?: AbortSignal) =>
    getJson<Article>(`/api/article/${id}${query({ edition: ed(edition) })}`, s),
  place: (gid: number | string, s?: AbortSignal) => getJson<PlaceResponse>(`/api/place/${gid}`, s),
  search: (
    p: { q: string; kind?: string; edition?: Edition; limit?: number; offset?: number },
    s?: AbortSignal,
  ) => getJson<SearchResponse>(`/api/search${query({ ...p, edition: p.edition })}`, s),
  suggest: (p: { q: string; limit?: number; edition?: Edition; kind?: string }, s?: AbortSignal) =>
    getJson<SuggestResponse>(`/api/suggest${query(p)}`, s),
  spell: (p: { q: string; edition?: Edition }, s?: AbortSignal) => getJson<SpellResponse>(`/api/spell${query(p)}`, s),
  dictionaries: (s?: AbortSignal) => getJson<{ dictionaries: DictionaryInfo[] }>(`/api/dictionary`, s),
  dictLookup: (
    p: { q: string; dict?: string; kind?: string; prefix?: boolean; limit?: number; around?: number },
    s?: AbortSignal,
  ) => getJson<DictLookupResponse>(`/api/dictionary/lookup${query(p)}`, s),
  dictEntry: (dict: string, id: number | string, s?: AbortSignal) =>
    getJson<DictEntry>(`/api/dictionary/${encodeURIComponent(dict)}/${id}`, s),
  /** An entry by ref id alone (ids are unique across the dictionaries). */
  dictEntryById: (id: number | string, s?: AbortSignal) => getJson<DictEntry>(`/api/dictionary/entry/${id}`, s),
  interactive: (fileName: string, edition?: Edition, s?: AbortSignal) =>
    getJson<Interactive>(`/api/interactive/${encodeURIComponent(fileName)}${query({ edition: ed(edition) })}`, s),
  timelineMeta: (s?: AbortSignal) => getJson<TimelineMeta>(`/api/timeline/meta`, s),
  timeline: (
    p: {
      from: number;
      to: number;
      category?: string;
      region?: string;
      q?: string;
      min_importance?: number;
      limit?: number;
      offset?: number;
      order?: "date" | "importance";
      text?: boolean;
    },
    s?: AbortSignal,
  ) => getJson<TimelineResponse>(`/api/timeline${query(p)}`, s),
  tours: (s?: AbortSignal) => getJson<ToursIndex>(`/out/tours/index.json`, s),
};
