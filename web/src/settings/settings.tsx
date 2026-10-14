/**
 * Reader settings (Tools > Options in Encarta): a small store kept in localStorage and shared
 * through React context. Storage may be missing or throw (private windows), so every access is
 * guarded and the settings then live for the session only.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type TextSize = "small" | "medium" | "large";
export type LatLongFormat = "dm" | "dms" | "decimal";
export type Units = "metric" | "us";

export interface Settings {
  startupSound: boolean;
  menuSounds: boolean;
  textSize: TextSize;
  captions: boolean;
  atlasLinearStats: boolean;
  atlasCrosshairs: boolean;
  latLongFormat: LatLongFormat;
  units: Units;
}

export const SETTINGS_KEY = "encarta.settings.v1";

export const DEFAULT_SETTINGS: Settings = {
  startupSound: true,
  menuSounds: true,
  textSize: "medium",
  captions: false,
  atlasLinearStats: false,
  atlasCrosshairs: true,
  latLongFormat: "dm",
  units: "us",
};

const bool = (v: unknown, d: boolean) => (typeof v === "boolean" ? v : d);
const oneOf = <T extends string>(v: unknown, all: readonly T[], d: T): T => (all.includes(v as T) ? (v as T) : d);

/** Parses stored settings; anything missing or malformed falls back to its default. */
export function parseSettings(raw: string | null): Settings {
  let o: Record<string, unknown> = {};
  try {
    const v: unknown = raw ? JSON.parse(raw) : {};
    if (v && typeof v === "object" && !Array.isArray(v)) o = v as Record<string, unknown>;
  } catch {
    /* defaults */
  }
  const d = DEFAULT_SETTINGS;
  return {
    startupSound: bool(o.startupSound, d.startupSound),
    menuSounds: bool(o.menuSounds, d.menuSounds),
    textSize: oneOf(o.textSize, ["small", "medium", "large"], d.textSize),
    captions: bool(o.captions, d.captions),
    atlasLinearStats: bool(o.atlasLinearStats, d.atlasLinearStats),
    atlasCrosshairs: bool(o.atlasCrosshairs, d.atlasCrosshairs),
    latLongFormat: oneOf(o.latLongFormat, ["dm", "dms", "decimal"], d.latLongFormat),
    units: oneOf(o.units, ["metric", "us"], d.units),
  };
}

export function loadSettings(): Settings {
  try {
    return parseSettings(window.localStorage.getItem(SETTINGS_KEY));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: Settings) {
  try {
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    /* session only */
  }
}

/** Mirrors the text size on the root element for the article CSS (`html[data-text-size]`). */
export function applyTextSize(size: TextSize) {
  if (typeof document !== "undefined") document.documentElement.dataset.textSize = size;
}

interface Ctx {
  settings: Settings;
  /** Replaces several settings at once and stores them. */
  update(patch: Partial<Settings>): void;
}

const SettingsContext = createContext<Ctx>({ settings: DEFAULT_SETTINGS, update: () => {} });

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      saveSettings(next);
      return next;
    });
  }, []);
  useEffect(() => applyTextSize(settings.textSize), [settings.textSize]);
  const value = useMemo(() => ({ settings, update }), [settings, update]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Ctx {
  return useContext(SettingsContext);
}
