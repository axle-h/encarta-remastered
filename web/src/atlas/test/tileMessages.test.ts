import { describe, expect, it } from "vitest";
import { TilePool, createWorkerHandler, type FromWorker, type RunJob, type ToWorker, type WorkerLike } from "../tileMessages";

type Job = { n: number };

/** A worker that loops messages straight into a worker handler (synchronously posted, asynchronously answered). */
function loopback(run: RunJob<Job>, log: ToWorker<Job>[] = []): WorkerLike {
  const listeners: ((e: { data: FromWorker }) => void)[] = [];
  const errors: ((e: unknown) => void)[] = [];
  const handle = createWorkerHandler<Job>(run, (m) => queueMicrotask(() => listeners.forEach((l) => l({ data: m }))));
  const w: WorkerLike & { fail(): void } = {
    postMessage: (m) => {
      log.push(m as ToWorker<Job>);
      handle(m as ToWorker<Job>);
    },
    addEventListener: ((type: string, l: never) => (type === "message" ? listeners : errors).push(l)) as WorkerLike["addEventListener"],
    terminate: () => {},
    fail: () => errors.forEach((l) => l({})),
  };
  return w;
}

const bytes = (n: number): ArrayBuffer => new Uint8Array([n]).buffer;
const echo: RunJob<Job> = async (job) => ({ data: bytes(job.n) });
const never: RunJob<Job> = (_job, signal) => new Promise((_, reject) => signal.addEventListener("abort", () => reject(new Error("stopped"))));

describe("tile worker messages", () => {
  it("answers each job with its own result", async () => {
    const pool = new TilePool<Job>(() => loopback(echo), 2, echo);
    const rs = await Promise.all([1, 2, 3].map((n) => pool.run({ n }, new AbortController().signal)));
    expect(rs.map((r) => new Uint8Array(r.data as ArrayBuffer)[0])).toEqual([1, 2, 3]);
  });

  it("starts workers on demand up to the pool size and spreads load", async () => {
    const pool = new TilePool<Job>(() => loopback(never), 3, echo);
    const ac = new AbortController();
    for (let i = 0; i < 5; i++) pool.run({ n: i }, ac.signal).catch(() => {});
    expect(pool.started).toBe(3);
    ac.abort();
  });

  it("forwards an abort to the worker and rejects with AbortError", async () => {
    const log: ToWorker<Job>[] = [];
    const pool = new TilePool<Job>(() => loopback(never, log), 1, echo);
    const ac = new AbortController();
    const p = pool.run({ n: 1 }, ac.signal);
    ac.abort();
    await expect(p).rejects.toMatchObject({ name: "AbortError" });
    expect(log.some((m) => "abort" in m && m.id === 1)).toBe(true);
  });

  it("rejects at once for a signal that is already aborted", async () => {
    const pool = new TilePool<Job>(() => loopback(echo), 1, echo);
    const ac = new AbortController();
    ac.abort();
    await expect(pool.run({ n: 1 }, ac.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(pool.started).toBe(0);
  });

  it("passes a job's error message back", async () => {
    const boom: RunJob<Job> = async () => {
      throw new Error("boom");
    };
    const pool = new TilePool<Job>(() => loopback(boom), 1, echo);
    await expect(pool.run({ n: 1 }, new AbortController().signal)).rejects.toThrow("boom");
  });

  it("does not post a result for an aborted job", async () => {
    const posted: FromWorker[] = [];
    const handle = createWorkerHandler<Job>(
      (_j, signal) => new Promise((resolve) => setTimeout(() => resolve({ data: bytes(1) }), 5).unref?.() ?? signal),
      (m) => posted.push(m),
    );
    handle({ id: 7, job: { n: 1 } });
    handle({ id: 7, abort: true });
    await new Promise((r) => setTimeout(r, 20));
    expect(posted).toEqual([]);
  });

  it("runs jobs on the calling thread when there is no Worker", async () => {
    const pool = new TilePool<Job>(null, 2, echo);
    const r = await pool.run({ n: 9 }, new AbortController().signal);
    expect(new Uint8Array(r.data as ArrayBuffer)[0]).toBe(9);
    expect(pool.started).toBe(0);
  });

  it("re-runs waiting jobs on the calling thread when a worker fails", async () => {
    let w!: ReturnType<typeof loopback> & { fail(): void };
    const pool = new TilePool<Job>(() => (w = loopback(never) as typeof w), 1, echo);
    const p = pool.run({ n: 4 }, new AbortController().signal);
    w.fail();
    expect(new Uint8Array(((await p).data as ArrayBuffer))[0]).toBe(4);
    const again = await pool.run({ n: 5 }, new AbortController().signal);
    expect(new Uint8Array(again.data as ArrayBuffer)[0]).toBe(5);
  });
});
