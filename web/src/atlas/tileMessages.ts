/**
 * The message protocol between the main thread and the tile workers, and the pool that speaks it.
 *
 * Main → worker: `{ id, job }` starts a job, `{ id, abort: true }` drops it. Worker → main:
 * `{ id, ok: true, result }` (the bytes or bitmap travel as transferables) or `{ id, ok: false, error }`.
 * Where `Worker` is unavailable, or the worker script fails to load, jobs run through `fallback`
 * on the calling thread.
 */

export interface TileResult {
  data: ArrayBuffer | ImageBitmap;
  cacheControl?: string;
  expires?: string;
}

export type ToWorker<J> = { id: number; job: J } | { id: number; abort: true };
export type FromWorker = { id: number; ok: true; result: TileResult } | { id: number; ok: false; error: string };

export type RunJob<J> = (job: J, signal: AbortSignal) => Promise<TileResult>;

/** The part of `Worker` the pool uses. */
export interface WorkerLike {
  postMessage(message: unknown, transfer?: Transferable[]): void;
  addEventListener(type: "message", listener: (e: { data: FromWorker }) => void): void;
  addEventListener(type: "error", listener: (e: unknown) => void): void;
  terminate(): void;
}

function transferList(r: TileResult): Transferable[] {
  return [r.data as Transferable];
}

/** The worker side: returns the `message` handler for a script that runs jobs with `run`. */
export function createWorkerHandler<J>(run: RunJob<J>, post: (m: FromWorker, transfer: Transferable[]) => void): (m: ToWorker<J>) => void {
  const running = new Map<number, AbortController>();
  return (m) => {
    if ("abort" in m) {
      running.get(m.id)?.abort();
      return;
    }
    const ac = new AbortController();
    running.set(m.id, ac);
    run(m.job, ac.signal)
      .then(
        (result) => {
          if (!ac.signal.aborted) post({ id: m.id, ok: true, result }, transferList(result));
        },
        (e: unknown) => {
          if (!ac.signal.aborted) post({ id: m.id, ok: false, error: e instanceof Error ? e.message : String(e) }, []);
        },
      )
      .finally(() => running.delete(m.id));
  };
}

interface Pending {
  resolve: (r: TileResult) => void;
  reject: (e: unknown) => void;
  worker: number;
  job: unknown;
  signal: AbortSignal;
  onAbort: () => void;
}

function abortError(): Error {
  const e = new Error("aborted");
  e.name = "AbortError";
  return e;
}

export class TilePool<J> {
  private workers: WorkerLike[] = [];
  private load: number[] = [];
  private pending = new Map<number, Pending>();
  private next = 1;
  private broken = false;

  constructor(
    private readonly make: (() => WorkerLike) | null,
    private readonly size: number,
    private readonly fallback: RunJob<J>,
  ) {}

  /** Number of workers started so far. */
  get started(): number {
    return this.workers.length;
  }

  run(job: J, signal: AbortSignal): Promise<TileResult> {
    if (signal.aborted) return Promise.reject(abortError());
    if (this.broken || !this.make) return this.fallback(job, signal);
    let w: number;
    try {
      w = this.pick();
    } catch {
      this.broken = true;
      return this.fallback(job, signal);
    }
    const id = this.next++;
    return new Promise<TileResult>((resolve, reject) => {
      const onAbort = () => {
        if (!this.pending.delete(id)) return;
        this.load[w]!--;
        this.workers[w]!.postMessage({ id, abort: true } satisfies ToWorker<J>);
        reject(abortError());
      };
      signal.addEventListener("abort", onAbort, { once: true });
      this.pending.set(id, { resolve, reject, worker: w, job, signal, onAbort });
      this.load[w]!++;
      this.workers[w]!.postMessage({ id, job } satisfies ToWorker<J>);
    });
  }

  terminate(): void {
    for (const w of this.workers) w.terminate();
    this.workers = [];
    this.load = [];
  }

  private pick(): number {
    let best = this.load.indexOf(Math.min(...this.load));
    if (this.workers.length < this.size && (best < 0 || this.load[best]! > 0)) {
      const index = this.workers.length;
      const worker = this.make!();
      worker.addEventListener("message", (e) => this.onMessage(e.data));
      worker.addEventListener("error", () => this.onError(index));
      this.workers.push(worker);
      this.load.push(0);
      best = index;
    }
    return best;
  }

  private onMessage(m: FromWorker): void {
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    this.load[p.worker]!--;
    p.signal.removeEventListener("abort", p.onAbort);
    if (m.ok) p.resolve(m.result);
    else p.reject(new Error(m.error));
  }

  /** The worker script failed: stop using workers and run what was waiting on this thread. */
  private onError(_index: number): void {
    if (this.broken) return;
    this.broken = true;
    const waiting = [...this.pending.values()];
    this.pending.clear();
    this.terminate();
    for (const p of waiting) {
      p.signal.removeEventListener("abort", p.onAbort);
      this.fallback(p.job as J, p.signal).then(p.resolve, p.reject);
    }
  }
}
