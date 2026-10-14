/** Runs a tile job on the current thread: the worker's body, and the fallback where workers are unavailable. */
import { makeLookTile, type LookJob } from "./lookTile";
import { makeMillerRasterTile, makeMillerVectorTile, type MillerRasterJob, type MillerVectorJob } from "./millerTiles";
import type { TileResult } from "./tileMessages";

export type TileJob = MillerVectorJob | MillerRasterJob | LookJob;

export function runTileJob(job: TileJob, signal: AbortSignal): Promise<TileResult> {
  switch (job.kind) {
    case "miller-mvt":
      return makeMillerVectorTile(job, signal);
    case "miller-raster":
      return makeMillerRasterTile(job, signal);
    case "look":
      return makeLookTile(job, signal);
  }
}
