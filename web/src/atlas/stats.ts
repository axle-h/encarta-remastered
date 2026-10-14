/**
 * Statistics overlay (the Statistical style with a statistic chosen, VM shot V14): countries coloured
 * from an MSWORLD.SDB indicator. The colours are the style's own `Z - (STAT) Positive n` /
 * `Negative n` classes (n = 1 … MAX), which MOBB assigns to countries at run time; the tiles carry
 * no features of these classes, so the client colours the Sovereign areas through feature-state.
 *
 * How Encarta bins values into the n classes is not known yet (V14 shows a continuous light-to-dark
 * ramp); this maps values linearly from 0 to the extreme, positive and negative separately.
 */
import type { StyleSpecification } from "maplibre-gl";
import { layerClasses } from "./layers";

export interface StatRamp {
  positive: string[];
  negative: string[];
}

const STAT_NAME = /^Z - \(STAT\) (Positive|Negative) (\d+)/;

export function statRamp(style: StyleSpecification): StatRamp {
  const pos = new Map<number, string>();
  const neg = new Map<number, string>();
  for (const layer of style.layers) {
    if (layer.type !== "fill") continue;
    const colour = (layer.paint as Record<string, unknown> | undefined)?.["fill-color"];
    if (typeof colour !== "string") continue;
    for (const name of layerClasses(layer).names) {
      const m = STAT_NAME.exec(name);
      if (!m) continue;
      (m[1] === "Positive" ? pos : neg).set(Number(m[2]), colour);
    }
  }
  const sorted = (m: Map<number, string>) => [...m.entries()].sort((a, b) => a[0] - b[0]).map(([, c]) => c);
  return { positive: sorted(pos), negative: sorted(neg) };
}

/** Fallback ramp (light to dark orange, the V14 look) for styles without STAT classes. */
export const DEFAULT_RAMP: StatRamp = {
  positive: Array.from({ length: 16 }, (_, i) => {
    const t = i / 15;
    const r = Math.round(0xfb + (0xbd - 0xfb) * t);
    const g = Math.round(0xe3 + (0x3d - 0xe3) * t);
    const b = Math.round(0xc6 + (0x1f - 0xc6) * t);
    return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  }),
  negative: [],
};

const HEX = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;

/** Blends the ramp's lightest and darkest colours (the Options "linear scale"); the nearest class when they are not hex. */
function blend(list: string[], t: number): string | null {
  const a = HEX.exec(list[0] ?? "");
  const b = HEX.exec(list[list.length - 1] ?? "");
  if (!a || !b) return null;
  const k = Math.min(1, Math.max(0, t));
  const ch = (i: number) => Math.round(parseInt(a[i]!, 16) + (parseInt(b[i]!, 16) - parseInt(a[i]!, 16)) * k).toString(16).padStart(2, "0");
  return `#${ch(1)}${ch(2)}${ch(3)}`;
}

/** Colour of one value. Class 1 is the lightest (near 0), the last the extreme; `linear` blends continuously instead. */
export function statColour(value: number, ramp: StatRamp, maxPos: number, minNeg: number, linear = false): string | null {
  if (!Number.isFinite(value)) return null;
  const pick = (list: string[], t: number) => (list.length ? (linear ? blend(list, t) : null) ?? list[Math.min(list.length - 1, Math.max(0, Math.floor(t * list.length)))]! : null);
  if (value >= 0 || !ramp.negative.length) {
    const t = maxPos > 0 ? Math.max(0, value) / maxPos : 0;
    return pick(ramp.positive, t);
  }
  const t = minNeg < 0 ? value / minNeg : 0;
  return pick(ramp.negative, t);
}

export function colourValues(values: readonly { country_id: number; value?: number | null }[], ramp: StatRamp, linear = false): Map<number, string> {
  let maxPos = 0;
  let minNeg = 0;
  for (const v of values) {
    if (typeof v.value !== "number") continue;
    if (v.value > maxPos) maxPos = v.value;
    if (v.value < minNeg) minNeg = v.value;
  }
  const out = new Map<number, string>();
  for (const v of values) {
    if (typeof v.value !== "number") continue;
    const c = statColour(v.value, ramp, maxPos, minNeg, linear);
    if (c) out.set(v.country_id, c);
  }
  return out;
}
