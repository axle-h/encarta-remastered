/**
 * MindMaze artwork: the `.dib` pictures of MINDMAZE.EIT, served as PNG derivatives. Sprites use
 * pure cyan (#00FFFF) as the transparent colour, so they are colour-keyed once on a canvas.
 */
import type { MediaInfo } from "../../api/types";
import { getMazeAssets, losslessUrl } from "../api";

export type Assets = Record<string, MediaInfo>;

let assetsPromise: Promise<Assets> | null = null;

export function loadAssets(): Promise<Assets> {
  assetsPromise ??= getMazeAssets().then((r) => r.assets);
  assetsPromise.catch(() => (assetsPromise = null));
  return assetsPromise;
}

/** Pictures that are drawn whole (no colour key). */
const OPAQUE = new Set([
  "titlescr.dib", "logonscr.dib", "plnwalls.dib", "dunrm.dib", "rmofdoor.dib", "atrium.dib", "grating.dib", "doortow.dib",
  "open1.dib", "open2.dib", "end1.dib", "end2.dib", "end3.dib", "end4.dib", "end5.dib", "end6.dib", "award.dib",
]);

/** True for a cyan key pixel (a little tolerance for any rounding in the derivative). */
export function isKey(r: number, g: number, b: number): boolean {
  return r <= 8 && g >= 247 && b >= 247;
}

/** Makes key pixels transparent in an RGBA buffer; returns how many changed. */
export function keyOut(data: Uint8ClampedArray): number {
  let n = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (isKey(data[i]!, data[i + 1]!, data[i + 2]!)) {
      data[i + 3] = 0;
      n++;
    }
  }
  return n;
}

const keyed = new Map<string, Promise<string>>();

/** URL of a picture ready to draw: the PNG itself for opaque art, a colour-keyed blob URL for sprites. */
export function artUrl(assets: Assets, name: string): Promise<string> {
  const info = assets[name];
  const url = losslessUrl(info) ?? `/media/file/${name}`;
  if (OPAQUE.has(name) || typeof document === "undefined") return Promise.resolve(url);
  let p = keyed.get(name);
  if (!p) {
    p = new Promise<string>((resolve) => {
      const img = new Image();
      img.onload = () => {
        try {
          const c = document.createElement("canvas");
          c.width = img.naturalWidth;
          c.height = img.naturalHeight;
          const ctx = c.getContext("2d");
          if (!ctx) return resolve(url);
          ctx.drawImage(img, 0, 0);
          const px = ctx.getImageData(0, 0, c.width, c.height);
          keyOut(px.data);
          ctx.putImageData(px, 0, 0);
          c.toBlob((b) => resolve(b ? URL.createObjectURL(b) : url), "image/png");
        } catch {
          resolve(url);
        }
      };
      img.onerror = () => resolve(url);
      img.src = url;
    });
    keyed.set(name, p);
  }
  return p;
}

export function artSize(assets: Assets, name: string): { w: number; h: number } {
  const v = assets[name]?.variants;
  const o = v?.png ?? v?.original;
  return { w: o?.width ?? 0, h: o?.height ?? 0 };
}
