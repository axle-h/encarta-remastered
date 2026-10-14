/**
 * Making the flat view's tiles (miller.ts "Tiles are re-made for the view coordinates by two
 * protocols"): the work behind `encarta-miller-mvt://` and `encarta-miller-raster://`. It fetches
 * its sources itself by absolute URL, so it runs in a worker (tile.worker.ts) or, as a fallback, on
 * the main thread (tileJobs.ts).
 */
import { PMTiles } from "pmtiles";
import { decodeTile, encodeTile, type Layer } from "./mvt";
import { MERCATOR_MAX_LAT, MILLER_IMAGE_MAX_ZOOM, TILE, mercWorldY, millerImageRect, realLatOfViewY, realRows, realZoom, retile } from "./miller";
import type { TileResult } from "./tileMessages";

export interface MillerVectorJob {
  kind: "miller-mvt";
  /** Absolute URL of the vector archive. */
  archive: string;
  /** The archive's own maximum zoom. */
  maxzoom: number;
  z: number;
  x: number;
  y: number;
}

export interface MillerRasterJob {
  kind: "miller-raster";
  /** Absolute URL of the Mercator raster archive. */
  archive: string;
  /** Absolute URL of Encarta's flat image for this zoom, if there is one. */
  imageUrl: string | null;
  z: number;
  x: number;
  y: number;
}

const archives = new Map<string, PMTiles>();
function archive(url: string): PMTiles {
  let a = archives.get(url);
  if (!a) archives.set(url, (a = new PMTiles(url)));
  return a;
}

export async function makeMillerVectorTile(t: MillerVectorJob, signal: AbortSignal): Promise<TileResult> {
  const a = archive(t.archive);
  const zr = realZoom(t.z, t.y, t.maxzoom);
  const shift = t.z - zr;
  const rx = t.x >> Math.max(0, shift);
  const [r0, r1] = realRows(t.z, t.y, zr);
  const sources: { zr: number; x: number; y: number; layers: Layer[] }[] = [];
  await Promise.all(
    Array.from({ length: r1 - r0 + 1 }, async (_, i) => {
      const tile = await a.getZxy(zr, rx, r0 + i, signal);
      if (tile?.data) sources.push({ zr, x: rx, y: r0 + i, layers: decodeTile(new Uint8Array(tile.data)) });
    }),
  );
  const layers = retile(sources, t.z, t.x, t.y);
  const bytes = encodeTile(layers);
  return { data: bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer };
}

const bitmaps = new Map<string, Promise<ImageBitmap | null>>();
function loadImage(url: string): Promise<ImageBitmap | null> {
  let p = bitmaps.get(url);
  if (!p) {
    p = fetch(url)
      .then((r) => (r.ok ? r.blob() : Promise.reject(new Error(String(r.status)))))
      .then((b) => createImageBitmap(b))
      .catch(() => null);
    bitmaps.set(url, p);
  }
  return p;
}

export async function makeMillerRasterTile(t: MillerRasterJob, signal: AbortSignal): Promise<TileResult> {
  const canvas = new OffscreenCanvas(TILE, TILE);
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  // Encarta's flat image first: all of the tile at small zooms, the polar rows at large zooms.
  if (t.imageUrl) {
    const img = await loadImage(t.imageUrl);
    if (img) {
      const r = millerImageRect(img.width, img.height, t.z, t.x, t.y);
      drawClipped(ctx, img, r.sx, r.sy, r.size, r.size);
    }
  }
  if (t.z >= MILLER_IMAGE_MAX_ZOOM || !t.imageUrl) {
    const a = archive(t.archive);
    const header = await a.getHeader();
    const zr = realZoom(t.z, t.y, header.maxZoom, 6);
    const shift = t.z - zr;
    const span = TILE / 2 ** shift; // source pixels per view tile, horizontally
    const rx = t.x >> shift;
    const sx = (t.x - (rx << shift)) * span;
    const [r0, r1] = realRows(t.z, t.y, zr);
    const tiles = new Map<number, ImageBitmap>();
    await Promise.all(
      Array.from({ length: r1 - r0 + 1 }, async (_, i) => {
        const tile = await a.getZxy(zr, rx, r0 + i, signal);
        if (tile?.data) tiles.set(r0 + i, await createImageBitmap(new Blob([tile.data])));
      }),
    );
    const n = 2 ** t.z;
    const m = 2 ** zr;
    for (let row = 0; row < TILE; row++) {
      const lat = realLatOfViewY((t.y + (row + 0.5) / TILE) / n);
      if (Math.abs(lat) > MERCATOR_MAX_LAT) continue;
      const sy = mercWorldY(lat) * m * TILE;
      const ty = Math.floor(sy / TILE);
      const img = tiles.get(ty);
      if (!img) continue;
      ctx.drawImage(img, sx, Math.floor(sy - ty * TILE), span, 1, 0, row, TILE, 1);
    }
    for (const b of tiles.values()) b.close();
  }
  const blob = await canvas.convertToBlob({ type: "image/png" });
  return { data: await blob.arrayBuffer() };
}

/** `drawImage` of a source rectangle that may extend beyond the image, onto the whole tile. */
function drawClipped(ctx: OffscreenCanvasRenderingContext2D, img: ImageBitmap, sx: number, sy: number, sw: number, sh: number): void {
  const x0 = Math.max(0, sx);
  const y0 = Math.max(0, sy);
  const x1 = Math.min(img.width, sx + sw);
  const y1 = Math.min(img.height, sy + sh);
  if (x1 <= x0 || y1 <= y0) return;
  const k = TILE / sw;
  ctx.drawImage(img, x0, y0, x1 - x0, y1 - y0, (x0 - sx) * k, (y0 - sy) * k, (x1 - x0) * k, (y1 - y0) * k);
}
