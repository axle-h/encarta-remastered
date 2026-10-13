/**
 * Timeline layout (pure, unit-tested): eras for quick navigation, axis ticks, and lane
 * assignment so event labels in the visible window do not overlap.
 */
import type { TimelineEvent } from "../api/types";

export interface Era {
  label: string;
  from: number;
  to: number;
}

/** Quick-jump eras; geologic time is far longer than history, so the windows are uneven. */
export const ERAS: Era[] = [
  { label: "Origins", from: -4_700_000_000, to: -500_000_000 },
  { label: "Age of Life", from: -500_000_000, to: -2_000_000 },
  { label: "Early Humans", from: -2_000_000, to: -10_000 },
  { label: "Ancient World", from: -10_000, to: 500 },
  { label: "Middle Ages", from: 500, to: 1500 },
  { label: "Early Modern", from: 1500, to: 1800 },
  { label: "19th Century", from: 1800, to: 1900 },
  { label: "20th Century", from: 1900, to: 2010 },
];

/** A "nice" tick step (1, 2, 5 × 10^n) giving about `count` ticks over `span` years. */
export function tickStep(span: number, count = 8): number {
  const raw = Math.max(1, span / count);
  const p = 10 ** Math.floor(Math.log10(raw));
  for (const m of [1, 2, 5, 10]) if (m * p >= raw) return m * p;
  return 10 * p;
}

export function ticks(from: number, to: number, count = 8): number[] {
  const step = tickStep(to - from, count);
  const out: number[] = [];
  for (let y = Math.ceil(from / step) * step; y <= to; y += step) out.push(y);
  return out;
}

export interface Placed {
  ev: TimelineEvent;
  x: number;
  lane: number;
}

/**
 * Places events (most important first) in the first lane where their label (`labelWidth` px from
 * x) does not overlap. Events that do not fit in `maxLanes` are returned in `hidden`.
 */
export function placeEvents(
  events: TimelineEvent[],
  from: number,
  to: number,
  width: number,
  labelWidth = 170,
  maxLanes = 9,
): { placed: Placed[]; hidden: TimelineEvent[] } {
  const span = Math.max(1e-9, to - from);
  const order = [...events].sort((a, b) => (b.importance ?? 0) - (a.importance ?? 0) || a.start.year - b.start.year);
  const lanes: [number, number][][] = [];
  const placed: Placed[] = [];
  const hidden: TimelineEvent[] = [];
  for (const ev of order) {
    const x = ((ev.start.year - from) / span) * width;
    const a = x;
    const b = x + labelWidth;
    let lane = -1;
    for (let i = 0; i < maxLanes; i++) {
      const used = (lanes[i] ??= []);
      if (used.every(([u, v]) => b <= u || a >= v)) {
        lane = i;
        used.push([a, b]);
        break;
      }
    }
    if (lane < 0) hidden.push(ev);
    else placed.push({ ev, x, lane });
  }
  placed.sort((p, q) => p.x - q.x);
  return { placed, hidden };
}

/** Zoom the window around its centre (factor < 1 zooms in), clamped to the data's range. */
export function zoom(from: number, to: number, factor: number, min: number, max: number): [number, number] {
  const c = (from + to) / 2;
  let half = ((to - from) / 2) * factor;
  half = Math.max(5, Math.min(half, (max - min) / 2));
  let a = c - half;
  let b = c + half;
  if (a < min) [a, b] = [min, min + 2 * half];
  if (b > max) [a, b] = [max - 2 * half, max];
  return [Math.round(a), Math.round(b)];
}

export function pan(from: number, to: number, frac: number, min: number, max: number): [number, number] {
  const d = (to - from) * frac;
  let a = from + d;
  let b = to + d;
  if (a < min) [a, b] = [min, min + (to - from)];
  if (b > max) [a, b] = [max - (to - from), max];
  return [Math.round(a), Math.round(b)];
}

/** Category colour index (0..5) from the first category bit of an event. */
export function categoryIndex(mask: number | undefined): number {
  if (!mask) return 6;
  for (let i = 0; i < 6; i++) if (mask & (1 << i)) return i;
  return 6;
}
