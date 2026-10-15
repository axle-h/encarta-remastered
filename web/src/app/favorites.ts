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
  /** Name of the folder it is filed in; absent for the top level ("Favorites"). */
  folder?: string;
}

/** Folders are one level deep (the menu cascades one level). */
export const FOLDERS_KEY = "encarta.favorite-folders.v1";

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
      .map((x) => ({
        path: x.path,
        title: typeof x.title === "string" && x.title ? x.title : x.path,
        added: Number(x.added) || 0,
        ...(typeof x.folder === "string" && x.folder ? { folder: x.folder } : {}),
      }));
  } catch {
    return [];
  }
}

/** Adds a page (or renames it when already there); newest first. */
export function addFavorite(list: Favorite[], path: string, title: string, now = Date.now(), folder?: string): Favorite[] {
  const t = title.trim() || path;
  const where = folder ? { folder } : {};
  if (list.some((f) => f.path === path)) {
    return list.map((f) => {
      if (f.path !== path) return f;
      const { folder: _old, ...rest } = f;
      return { ...rest, title: t, ...where };
    });
  }
  return [{ path, title: t, added: now, ...where }, ...list].slice(0, MAX);
}

/** Files a favorite in a folder (undefined: the top level). */
export function moveToFolder(list: Favorite[], path: string, folder?: string): Favorite[] {
  return list.map((f) => {
    if (f.path !== path) return f;
    const { folder: _old, ...rest } = f;
    return folder ? { ...rest, folder } : rest;
  });
}

/** Parses the stored folder names: non-empty, unique strings. */
export function parseFolders(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const v: unknown = JSON.parse(raw);
    if (!Array.isArray(v)) return [];
    return [...new Set(v.filter((x): x is string => typeof x === "string" && x.trim() !== "").map((x) => x.trim()))];
  } catch {
    return [];
  }
}

/** The folder names that exist: the stored ones plus any a favorite is filed in. */
export function allFolders(folders: string[], list: Favorite[]): string[] {
  return [...new Set([...folders, ...list.flatMap((f) => (f.folder ? [f.folder] : []))])];
}

/** A new folder; null when the name is empty or taken (case-insensitive, as in Windows). */
export function addFolder(folders: string[], name: string): string[] | null {
  const n = name.trim();
  if (!n || folders.some((f) => f.toLowerCase() === n.toLowerCase())) return null;
  return [...folders, n];
}

/** Renames a folder and re-files its favorites; null when the new name is empty or taken. */
export function renameFolder(folders: string[], list: Favorite[], from: string, to: string): { folders: string[]; list: Favorite[] } | null {
  const n = to.trim();
  if (!n || (n.toLowerCase() !== from.toLowerCase() && folders.some((f) => f.toLowerCase() === n.toLowerCase()))) return null;
  return {
    folders: folders.map((f) => (f === from ? n : f)),
    list: list.map((f) => (f.folder === from ? { ...f, folder: n } : f)),
  };
}

/** Deletes a folder with the favorites in it (a folder is a directory in Encarta). */
export function removeFolder(folders: string[], list: Favorite[], name: string): { folders: string[]; list: Favorite[] } {
  return { folders: folders.filter((f) => f !== name), list: list.filter((f) => f.folder !== name) };
}

export function removeFavorite(list: Favorite[], path: string): Favorite[] {
  return list.filter((f) => f.path !== path);
}

/** Moves an entry up (-1) or down (+1) among the entries of its own folder. */
export function moveFavorite(list: Favorite[], path: string, by: -1 | 1): Favorite[] {
  const i = list.findIndex((f) => f.path === path);
  if (i < 0) return list;
  const folder = list[i]!.folder;
  let j = i + by;
  while (j >= 0 && j < list.length && list[j]!.folder !== folder) j += by;
  if (j < 0 || j >= list.length) return list;
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
let folderMemory: string[] | null = null;
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

function readFolders(): string[] {
  if (folderMemory) return folderMemory;
  let raw: string | null = null;
  try {
    raw = window.localStorage.getItem(FOLDERS_KEY);
  } catch {
    /* storage blocked: session-only folders */
  }
  folderMemory = parseFolders(raw);
  return folderMemory;
}

function save(key: string, value: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage blocked or full: keep them for this session */
  }
}

function write(list: Favorite[], folders?: string[]) {
  memory = list;
  save(FAVORITES_KEY, list);
  if (folders) {
    folderMemory = folders;
    save(FOLDERS_KEY, folders);
  }
  listeners.forEach((l) => l());
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  const onStorage = (e: StorageEvent) => {
    if (e.key === FAVORITES_KEY || e.key === FOLDERS_KEY) {
      memory = null;
      folderMemory = null;
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
  add: (path: string, title: string, folder?: string) => write(addFavorite(read(), path, title, Date.now(), folder)),
  /** Folder names (stored ones and those a favorite is filed in). */
  folders: () => allFolders(readFolders(), read()),
  /** Creates a folder; false when the name is empty or taken. */
  addFolder: (name: string): boolean => {
    const next = addFolder(allFolders(readFolders(), read()), name);
    if (!next) return false;
    write(read(), next);
    return true;
  },
  renameFolder: (from: string, to: string): boolean => {
    const r = renameFolder(allFolders(readFolders(), read()), read(), from, to);
    if (!r) return false;
    write(r.list, r.folders);
    return true;
  },
  removeFolder: (name: string) => {
    const r = removeFolder(allFolders(readFolders(), read()), read(), name);
    write(r.list, r.folders);
  },
  moveToFolder: (path: string, folder?: string) => write(moveToFolder(read(), path, folder)),
  remove: (path: string) => write(removeFavorite(read(), path)),
  move: (path: string, by: -1 | 1) => write(moveFavorite(read(), path, by)),
  rename: (path: string, title: string) => write(renameFavorite(read(), path, title)),
  /** Forgets the in-memory copy (tests). */
  reset: () => {
    memory = null;
    folderMemory = null;
  },
};

export function useFavorites(): Favorite[] {
  return useSyncExternalStore(subscribe, read, read);
}

/** The folder names, re-read when favorites or folders change. */
export function useFolders(): string[] {
  const list = useFavorites();
  const stored = useSyncExternalStore(subscribe, readFolders, readFolders);
  return allFolders(stored, list);
}
