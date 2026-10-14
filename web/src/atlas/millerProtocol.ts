/**
 * The `encarta-miller-mvt://` and `encarta-miller-raster://` protocol handlers (miller.ts). MapLibre
 * calls them on the main thread, so they only parse the URL and hand the job to a tile worker
 * (tilePool.ts), which fetches and re-makes the tile (millerTiles.ts).
 */
import type { AddProtocolAction } from "maplibre-gl";
import { MILLER_MVT_PROTOCOL, MILLER_RASTER_PROTOCOL, millerLevel, parseRasterUrl, parseVectorUrl, type MillerImage } from "./miller";
import { runInWorker } from "./tilePool";

export function createMillerVectorProtocol(): AddProtocolAction {
  return async (params, abort) => {
    const t = parseVectorUrl(params.url);
    if (!t) throw new Error(`bad ${MILLER_MVT_PROTOCOL} URL ${params.url}`);
    const r = await runInWorker({ kind: "miller-mvt", archive: t.archive, maxzoom: t.maxzoom, z: t.z, x: t.x, y: t.y }, abort.signal);
    return { data: r.data as ArrayBuffer };
  };
}

export function createMillerRasterProtocol(get: () => { images: MillerImage[]; imageUrl: (raster: string, look: string, level: number) => string }): AddProtocolAction {
  return async (params, abort) => {
    const t = parseRasterUrl(params.url);
    if (!t) throw new Error(`bad ${MILLER_RASTER_PROTOCOL} URL ${params.url}`);
    const { images, imageUrl } = get();
    const level = millerLevel(images.filter((m) => m.raster.toUpperCase() === t.raster), t.z);
    const url = level ? imageUrl(t.raster, t.look, level.level) : null;
    const r = await runInWorker({ kind: "miller-raster", archive: t.archive, imageUrl: url, z: t.z, x: t.x, y: t.y }, abort.signal);
    return { data: r.data as ArrayBuffer };
  };
}
