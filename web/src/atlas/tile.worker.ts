/** Tile worker: runs the jobs of tileJobs.ts off the main thread (tileMessages.ts protocol). */
import { createWorkerHandler } from "./tileMessages";
import { runTileJob, type TileJob } from "./tileJobs";

const scope = self as unknown as { postMessage(m: unknown, t: Transferable[]): void; onmessage: ((e: { data: never }) => void) | null };
const handle = createWorkerHandler<TileJob>(runTileJob, (m, transfer) => scope.postMessage(m, transfer));
scope.onmessage = (e) => handle(e.data);
