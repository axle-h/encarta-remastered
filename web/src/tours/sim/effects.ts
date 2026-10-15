/**
 * Time-driven effects advanced once per tick (the original advances them once per drawn frame):
 * light flicker (§7.2.6), 3D film strips (§7.4.4) and the water ripple (§7.4.6).
 */
import type { RGB } from "../types";

/** A uniform random source in [0, 1). */
export type Rng = () => number;

/** `rand() & 0x1FF` style: an integer in 0…2^bits−1. */
export const randBits = (rng: Rng, bits: number) => Math.floor(rng() * (1 << bits)) & ((1 << bits) - 1);

/** A seeded LCG for tests and reproducible runs (MSVC constants; sequence parity is not required). */
export function lcg(seed = 1): Rng {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 214013) + 2531011) >>> 0;
    return ((s >>> 16) & 0x7fff) / 32768;
  };
}

/** Flicker intensity factor: `(rand() & 0x1FF) / 511 × 0.6 + 0.4`, uniform in [0.4, 1.0] in 512 steps. */
export function flickerFactor(rng: Rng): number {
  return (randBits(rng, 9) / 511) * 0.6 + 0.4;
}

export class FlickerLight {
  /** Seconds between redraws of the factor. */
  period: number;
  /** Saved base colour, 0–255 per channel. */
  readonly saved: RGB;
  /** Current colour, integers 0–255; a step function between redraws. */
  current: RGB;
  private acc = 0;

  constructor(period: number, saved: RGB) {
    this.period = period;
    this.saved = [...saved];
    this.current = [...saved];
  }

  /** Adds elapsed time; at each period the factor is redrawn and held. Returns true on a change. */
  advance(dt: number, rng: Rng): boolean {
    this.acc += dt;
    if (this.acc < this.period) return false;
    this.acc = 0;
    const k = flickerFactor(rng);
    this.current = [scale(k, this.saved[0]), scale(k, this.saved[1]), scale(k, this.saved[2])];
    return true;
  }
}

/** `trunc(k × saved/255 × 255)` with a guard against float noise on exact products. */
const scale = (k: number, c: number) => Math.trunc(k * c + 1e-9);

/**
 * The vertex colour term of one flickering light (docs/tour-scene.md "Runtime terms"):
 * `min(255, base + Σ trunc(current · w))`.
 */
export function flickerColour(base: number, terms: readonly [current: number, weight: number][]): number {
  let c = base;
  for (const [cur, w] of terms) c += Math.trunc(cur * w);
  return Math.min(255, c);
}

/** A horizontal 3D film strip of N frames (§7.4.4). */
export class Filmstrip {
  readonly frames: number;
  wait: number;
  /** 1…N once running; 0 until the first advance shows frame 1 (U = savedU/N). */
  private frame: number;
  private acc = 0;
  /** Current U offset added to savedU/N. */
  offset = 0;

  constructor(frames: number, wait: number, rng: Rng) {
    this.frames = Math.max(1, frames);
    this.wait = wait;
    this.frame = Math.floor(rng() * 32768) % this.frames;
  }

  /** Per tick; returns true when the frame changed. */
  advance(dt: number): boolean {
    this.acc += dt;
    if (this.acc < this.wait) return false;
    this.acc = 0;
    this.frame = (this.frame % this.frames) + 1;
    this.offset = (this.frame - 1) / this.frames;
    return true;
  }
}

/** The 5×5 drop stamp without its corners (§7.4.6). */
export const DROP_STAMP: readonly (readonly number[])[] = [
  [0, 700, 500, 700, 0],
  [700, 1200, 1400, 1200, 700],
  [500, 1400, 1500, 1400, 500],
  [700, 1200, 1400, 1200, 700],
  [0, 700, 500, 700, 0],
];

/**
 * Frames left out after a ripple step or added to a drop wait (§7.4.6): `trunc(fps × 0.025) − 1` above
 * 40 fps, else 0. `fps` is the instantaneous rate, 1 / (duration of the last frame).
 */
export function rippleSkip(fps: number): number {
  return fps > 40 ? Math.trunc(fps * Math.fround(0.025)) - 1 : 0;
}

/**
 * The ripple simulation (§7.4.6) on a W×H height field pair, kept in surface order: row 0 is the
 * bottom row of the image (§5.6).
 */
export class Ripple {
  readonly w: number;
  readonly h: number;
  /** The field the next step starts from. */
  cur: Int32Array;
  /** The field before it; after a step, the one the display shows. */
  prev: Int32Array;
  private skip = 0;
  private skipped = 0;
  private dropWait = 0;
  private dropCount = 0;

  constructor(w: number, h: number) {
    this.w = w;
    this.h = h;
    this.cur = new Int32Array(w * h);
    this.prev = new Int32Array(w * h);
  }

  /**
   * One drawn frame at `fps`: steps unless the skip count from the last step says otherwise; each
   * step first tries a drop when its wait is over. Returns whether the field stepped (and the
   * picture changed).
   */
  frame(fps: number, rng: Rng): boolean {
    if (this.skip !== 0 && this.skipped++ < this.skip) return false;
    this.skipped = 0;
    this.skip = rippleSkip(fps);
    if (this.dropCount++ >= this.dropWait) {
      this.dropCount = 0;
      this.dropWait = randBits(rng, 1) + rippleSkip(fps);
      this.randomDrop(rng);
    }
    this.step();
    return true;
  }

  /**
   * One step for texels at least 2 from each edge: new = ((sum of 8 neighbours) >> 2) − older, then
   * new −= new >> 4 (arithmetic shifts); the fields swap.
   */
  step(): void {
    const { w, h, cur } = this;
    const out = this.prev;
    for (let y = 2; y < h - 2; y++) {
      for (let x = 2; x < w - 2; x++) {
        const i = y * w + x;
        const s =
          (cur[i - 1] ?? 0) + (cur[i + 1] ?? 0) + (cur[i - w] ?? 0) + (cur[i + w] ?? 0) +
          (cur[i - w - 1] ?? 0) + (cur[i - w + 1] ?? 0) + (cur[i + w - 1] ?? 0) + (cur[i + w + 1] ?? 0);
        const v = (s >> 2) - (out[i] ?? 0);
        out[i] = v - (v >> 4);
      }
    }
    this.prev = cur;
    this.cur = out;
  }

  /** Stamps a drop centred on surface texel (cx, cy) with every value shifted right by `shift`. */
  drop(cx: number, cy: number, shift: number): void {
    for (let dy = -2; dy <= 2; dy++) {
      for (let dx = -2; dx <= 2; dx++) {
        const v = DROP_STAMP[dy + 2]?.[dx + 2] ?? 0;
        if (!v) continue;
        const x = cx + dx, y = cy + dy;
        if (x < 0 || y < 0 || x >= this.w || y >= this.h) continue;
        this.cur[y * this.w + x] = v >> shift;
      }
    }
  }

  /** A drop try: x = rand mod W, y = rand mod H, stamped only when 3 < x < W − 3 and 3 < y < H − 3. */
  randomDrop(rng: Rng): boolean {
    const x = Math.floor(rng() * this.w);
    const y = Math.floor(rng() * this.h);
    if (x <= 3 || x >= this.w - 3 || y <= 3 || y >= this.h - 3) return false;
    this.drop(x, y, randBits(rng, 4) + 1);
    return true;
  }

  /**
   * The picture after a step: the field the step started from. Surface texel i (x ≥ 1, y ≥ 1) takes
   * the original at index i + (h[i−W] − h[i])·W + (h[i−1] − h[i]), a linear offset that runs on into the
   * neighbouring row; outside the bitmap gives black; row 0 and column 0 copy. `src` and `dst` are
   * RGBA images of W×H in display order (top row first), so surface row r is image row H − 1 − r.
   */
  render(src: Uint8ClampedArray | Uint8Array, dst: Uint8ClampedArray | Uint8Array): void {
    const { w, h } = this;
    const f = this.prev;
    const n = w * h;
    const image = (i: number) => ((h - 1 - Math.floor(i / w)) * w + (i % w)) * 4;
    for (let i = 0; i < n; i++) {
      const x = i % w;
      let s = i;
      if (x > 0 && i >= w) {
        const hv = f[i] ?? 0;
        s = i + ((f[i - w] ?? 0) - hv) * w + ((f[i - 1] ?? 0) - hv);
      }
      const o = image(i);
      if (s < 0 || s >= n) {
        dst[o] = 0; dst[o + 1] = 0; dst[o + 2] = 0; dst[o + 3] = 255;
        continue;
      }
      const so = image(s);
      dst[o] = src[so] ?? 0; dst[o + 1] = src[so + 1] ?? 0; dst[o + 2] = src[so + 2] ?? 0; dst[o + 3] = src[so + 3] ?? 255;
    }
  }
}

/**
 * Cloud impostor geometry (§7.3.3) in original (left-handed) coordinates: the view angle of the
 * eye around the cloud centre, and the chunk acceptance test.
 */
export function cloudPhi(centreX: number, centreZorig: number, eyeX: number, eyeZorig: number, offset: number): number {
  let phi = Math.atan2(centreZorig - eyeZorig, centreX - eyeX) + offset;
  const tau = Math.PI * 2;
  phi = ((phi % tau) + tau) % tau;
  return phi;
}

export const angleDiff = (a: number, b: number) => {
  const d = Math.abs(a - b);
  return Math.min(d, Math.abs(d - 360));
};

/** Whether a 500-point chunk with boundary keys k0, k1 is drawn (NaN keys, stored as −1, always are). */
export function chunkVisible(viewDeg: number, k0: number, k1: number): boolean {
  if (k0 < 0 || k1 < 0 || Number.isNaN(k0) || Number.isNaN(k1)) return true;
  return angleDiff(viewDeg, k0) <= 95 || angleDiff(viewDeg, k1) <= 95;
}

/**
 * Reference software rasteriser of one cloud into a W×H index image (−1 = keyed texel), as in
 * §7.3.3 (no per-row clipping; the last point is never drawn). Used by tests and as the spec for
 * the GPU impostor shader.
 */
export function rasteriseCloud(
  points: { x: number; y: number; z: number; key: number }[],
  s: number,
  phi: number,
  w: number,
  h: number,
): Int32Array {
  const out = new Int32Array(w * h).fill(-1);
  const zbuf = new Float64Array(w * h).fill(Infinity);
  const viewDeg = phi * 57.29578 - 90;
  const c = Math.cos(phi), sn = Math.sin(phi);
  const n = points.length;
  for (let i = 0; i < n - 1; i += 500) {
    const j = Math.min(i + 500, n - 1);
    if (!chunkVisible(viewDeg, points[i]?.key ?? -1, points[j]?.key ?? -1)) continue;
    for (let p = i; p < j; p++) {
      const pt = points[p];
      if (!pt) continue;
      const u0 = pt.x * s, z0 = pt.z * s;
      const col = Math.floor(u0 * c + z0 * sn);
      const row = Math.trunc(pt.y * s);
      const index = row * w + w / 2 + col;
      if (index < 0 || index >= w * h) continue;
      const depth = (z0 * c - u0 * sn) * 64 + 65536;
      if (depth < (zbuf[index] ?? Infinity)) {
        zbuf[index] = depth;
        out[index] = p;
      }
    }
  }
  return out;
}
