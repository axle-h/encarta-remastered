import { createContext, useContext, useEffect, useState } from "react";
import { dataFile, fetchJson, type Endpoints } from "../data";

export interface SpriteEntry {
  x: number;
  y: number;
  width: number;
  height: number;
  pixelRatio?: number;
}

export interface SpriteSheet {
  url: string;
  ratio: number;
  entries: Record<string, SpriteEntry>;
}

export const SpriteContext = createContext<SpriteSheet | null>(null);

/** The converter's sprite sheet (`sprites/sprite@2x`, the 149 Bitmap symbols as `bm<id>`). */
export function useSpriteSheet(ep: Endpoints): SpriteSheet | null {
  const [sheet, setSheet] = useState<SpriteSheet | null>(null);
  useEffect(() => {
    const ac = new AbortController();
    fetchJson<Record<string, SpriteEntry>>(dataFile("sprites/sprite@2x.json", ep), ac.signal)
      .then((entries) => setSheet({ url: dataFile("sprites/sprite@2x.png", ep), ratio: 2, entries }))
      .catch(() => undefined);
    return () => ac.abort();
  }, [ep]);
  return sheet;
}

/** One sprite at its 2x size (Encarta's bitmaps are 4–14 px; 2x is legible in lists). */
export function SpriteIcon({ name, title }: { name: string; title?: string }) {
  const sheet = useContext(SpriteContext);
  const e = sheet?.entries[name];
  if (!sheet || !e) return <span className="enc-atlas-icon enc-atlas-icon--empty" aria-hidden="true" />;
  return (
    <span
      className="enc-atlas-icon"
      role="img"
      aria-label={title ?? name}
      title={title}
      style={{
        width: e.width,
        height: e.height,
        backgroundImage: `url("${sheet.url}")`,
        backgroundPosition: `-${e.x}px -${e.y}px`,
      }}
    />
  );
}
