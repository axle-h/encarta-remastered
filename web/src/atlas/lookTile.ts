/** The pixel work of `encarta-look://` tiles (recolour.ts): decode an index tile and apply a palette. Runs in a worker or on the main thread. */
import { PMTiles } from "pmtiles";
import { decodePngValues } from "./png";
import type { TileResult } from "./tileMessages";

export interface LookJob {
  kind: "look";
  /** Absolute URL of the raster's `index` archive. */
  archive: string;
  /** CMT value → packed RGBA (recolour.ts `buildLut`). */
  lut: Uint32Array;
  z: number;
  x: number;
  y: number;
}

export function applyLut(values: Uint8Array, lut: Uint32Array, out = new Uint32Array(values.length)): Uint32Array {
  for (let i = 0; i < values.length; i++) out[i] = lut[values[i]!]!;
  return out;
}

const archives = new Map<string, PMTiles>();

export async function makeLookTile(job: LookJob, signal: AbortSignal): Promise<TileResult> {
  let a = archives.get(job.archive);
  if (!a) archives.set(job.archive, (a = new PMTiles(job.archive)));
  const tile = await a.getZxy(job.z, job.x, job.y, signal);
  if (!tile) return { data: await createImageBitmap(new ImageData(256, 256)) };
  const png = await decodePngValues(new Uint8Array(tile.data));
  const rgba = applyLut(png.values, job.lut);
  const image = new ImageData(new Uint8ClampedArray(rgba.buffer), png.width, png.height);
  return { data: await createImageBitmap(image), cacheControl: tile.cacheControl, expires: tile.expires };
}
