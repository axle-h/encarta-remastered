/**
 * Favorites (Encarta's Favorites menu): pages the reader saved, kept in this browser's
 * localStorage. Pure list operations plus a small store that React subscribes to; other tabs
 * see changes through the `storage` event. Storage may be missing or throw (private windows,
 * blocked site data), so every access is guarded and the menu still works for the session.
 */
import { useSyncExternalStore } from "react";

export interface Favorite {
  /** Client route with its query and hash, e.g. `/item/761575094#p12`. */
  path: string;
  title: string;
  /** Milliseconds since the epoch. */
  added: number;
}

export const FAVORITES_KEY = "encarta.favorites.v1";
const MAX = 500;

/** Parses stored favorites, dropping malformed entries. */
export function parseFavorites(raw: string | null): Favorite[] {
  if (!raw) return [];
  try {
    const v: unknown = JSON.parse(raw);
    if (!Array.isArray(v)) return [];
    return v
      .filter((x): x is Favorite => !!x && typeof x === "object" && typeof (x as Favorite).path === "string" && (x as Favorite).path.startsWith("/"))
      .map((x) => ({ path: x.path, title: typeof x.title === "string" && x.title ? x.title : x.path, added: Number(x.added) || 0 }));
  } catch {
    return [];
  }
}

/** Adds a page (or renames it when already there); newest first. */
export function addFavorite(list: Favorite[], path: string, title: string, now = Date.now()): Favorite[] {
  const t = title.trim() || path;
  if (list.some((f) => f.path === path)) return list.map((f) => (f.path === path ? { ...f, title: t } : f));
  return [{ path, title: t, added: now }, ...list].slice(0, MAX);
}

export function removeFavorite(list: Favorite[], path: string): Favorite[] {
  return list.filter((f) => f.path !== path);
}

/** Moves an entry up (-1) or down (+1). */
export function moveFavorite(list: Favorite[], path: string, by: -1 | 1): Favorite[] {
  const i = list.findIndex((f) => f.path === path);
  const j = i + by;
  if (i < 0 || j < 0 || j >= list.length) return list;
  const out = [...list];
  [out[i], out[j]] = [out[j]!, out[i]!];
  return out;
}

export function renameFavorite(list: Favorite[], path: string, title: string): Favorite[] {
  const t = title.trim();
  return t ? list.map((f) => (f.path === path ? { ...f, title: t } : f)) : list;
}

/** The page's own title for a favorite: `document.title` without the " - Encarta" suffix. */
export function pageTitle(docTitle: string): string {
  return docTitle.replace(/\s*[-–|]\s*Encarta.*$/i, "").trim() || "Encarta";
}

// ---------------------------------------------------------------- store

let memory: Favorite[] | null = null;
const listeners = new Set<() => void>();

function read(): Favorite[] {
  if (memory) return memory;
  let raw: string | null = null;
  try {
    raw = window.localStorage.getItem(FAVORITES_KEY);
  } catch {
    /* storage blocked: session-only favorites */
  }
  memory = parseFavorites(raw);
  return memory;
}

function write(list: Favorite[]) {
  memory = list;
  try {
    window.localStorage.setItem(FAVORITES_KEY, JSON.stringify(list));
  } catch {
    /* storage blocked or full: keep them for this session */
  }
  listeners.forEach((l) => l());
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  const onStorage = (e: StorageEvent) => {
    if (e.key === FAVORITES_KEY) {
      memory = null;
      l();
    }
  };
  window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(l);
    window.removeEventListener("storage", onStorage);
  };
}

export const favorites = {
  list: read,
  add: (path: string, title: string) => write(addFavorite(read(), path, title)),
  remove: (path: string) => write(removeFavorite(read(), path)),
  move: (path: string, by: -1 | 1) => write(moveFavorite(read(), path, by)),
  rename: (path: string, title: string) => write(renameFavorite(read(), path, title)),
  /** Forgets the in-memory copy (tests). */
  reset: () => {
    memory = null;
  },
};

export function useFavorites(): Favorite[] {
  return useSyncExternalStore(subscribe, read, read);
}
