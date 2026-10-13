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

/** The ripple simulation on a W×H height field pair. */
export class Ripple {
  readonly w: number;
  readonly h: number;
  cur: Int32Array;
  prev: Int32Array;

  constructor(w: number, h: number) {
    this.w = w;
    this.h = h;
    this.cur = new Int32Array(w * h);
    this.prev = new Int32Array(w * h);
  }

  /** One step: new = (sum of 8 neighbours)/4 − older, then new −= new/16; fields swap. */
  step(): void {
    const { w, h, cur } = this;
    const out = this.prev;
    for (let y = 1; y < h - 1; y++) {
      for (let x = 1; x < w - 1; x++) {
        const i = y * w + x;
        const s =
          (cur[i - 1] ?? 0) + (cur[i + 1] ?? 0) + (cur[i - w] ?? 0) + (cur[i + w] ?? 0) +
          (cur[i - w - 1] ?? 0) + (cur[i - w + 1] ?? 0) + (cur[i + w - 1] ?? 0) + (cur[i + w + 1] ?? 0);
        let v = Math.trunc(s / 4) - (out[i] ?? 0);
        v -= Math.trunc(v / 16);
        out[i] = v;
      }
    }
    this.prev = cur;
    this.cur = out;
  }

  /** Stamps a drop at (cx, cy) with every value shifted right by `shift`. */
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

  /** A drop at a random position at least 3 texels from the edges, shift `(rand() & 15) + 1`. */
  randomDrop(rng: Rng): void {
    if (this.w < 7 || this.h < 7) return;
    const x = 3 + Math.floor(rng() * (this.w - 6));
    const y = 3 + Math.floor(rng() * (this.h - 6));
    this.drop(x, y, randBits(rng, 4) + 1);
  }

  /**
   * Display: texel (x, y) = source texel displaced by (h[x−1] − h[x]) columns and (h[x−W] − h[x])
   * rows; outside the bitmap gives black. RGBA buffers of W×H.
   */
  render(src: Uint8ClampedArray | Uint8Array, dst: Uint8ClampedArray | Uint8Array): void {
    const { w, h, cur } = this;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const i = y * w + x;
        const hv = cur[i] ?? 0;
        const ox = (x > 0 ? (cur[i - 1] ?? 0) : hv) - hv;
        const oy = (y > 0 ? (cur[i - w] ?? 0) : hv) - hv;
        const sx = x + ox, sy = y + oy;
        const o = i * 4;
        if (sx < 0 || sy < 0 || sx >= w || sy >= h) {
          dst[o] = 0; dst[o + 1] = 0; dst[o + 2] = 0; dst[o + 3] = 255;
          continue;
        }
        const s = (sy * w + sx) * 4;
        dst[o] = src[s] ?? 0; dst[o + 1] = src[s + 1] ?? 0; dst[o + 2] = src[s + 2] ?? 0; dst[o + 3] = src[s + 3] ?? 255;
      }
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
