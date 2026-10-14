/** The page's pool of tile workers; the protocol handlers (millerProtocol.ts, recolour.ts) post their jobs here. */
import { TilePool, type TileResult, type WorkerLike } from "./tileMessages";
import { runTileJob, type TileJob } from "./tileJobs";

const canWork = () => typeof Worker !== "undefined" && typeof OffscreenCanvas !== "undefined" && typeof createImageBitmap === "function";

let pool: TilePool<TileJob> | null = null;

export function runInWorker(job: TileJob, signal: AbortSignal): Promise<TileResult> {
  if (!pool) {
    const size = Math.max(2, Math.min(4, (typeof navigator !== "undefined" ? navigator.hardwareConcurrency || 4 : 4) - 1));
    const make = canWork() ? () => new Worker(new URL("./tile.worker.ts", import.meta.url), { type: "module", name: "atlas-tiles" }) as unknown as WorkerLike : null;
    pool = new TilePool<TileJob>(make, size, runTileJob);
  }
  return pool.run(job, signal);
}
