/**
 * Chart arithmetic for City Climate and Population Chart (docs/games.md "Charts"), free of React.
 */

/** The four series colours (validated categorical slots 1–4), in fixed slot order. */
export const SERIES = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"];
export const BAR = "#2a78d6";

/** The smallest preset scale whose top covers `max` (the original's `pollForChartRange`); the largest otherwise. */
export function pickScale(scales: number[][], max: number): number[] {
  if (!scales.length) return [0, 1];
  for (const s of scales) if ((s[s.length - 1] ?? 0) >= max) return s;
  return scales[scales.length - 1]!;
}

/**
 * Round ticks from `min` to `max` (inclusive), about `target` of them. A step that divides the range
 * exactly is preferred, so the axis ends (the datasets' preset `axis_min`/`axis_max`) get ticks.
 */
export function niceTicks(min: number, max: number, target = 7): number[] {
  if (!(max > min)) return [min];
  const span = max - min;
  const raw = span / Math.max(1, target - 1);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const candidates = [0.5, 1, 2, 2.5, 5, 10, 20].map((m) => m * mag);
  const exact = candidates.filter((st) => Math.abs(span / st - Math.round(span / st)) < 1e-9 && span / st >= 3 && span / st <= 12);
  const best = exact.sort((a, b) => Math.abs(span / a - target) - Math.abs(span / b - target))[0];
  const step = best ?? candidates.find((st) => st >= raw) ?? raw;
  const out: number[] = [];
  const start = best ? min : Math.ceil(min / step - 1e-9) * step;
  for (let v = start; v <= max + 1e-9; v += step) out.push(Number(v.toFixed(10)));
  return out;
}

/** A linear map from [d0, d1] to [r0, r1]. */
export function linear(d0: number, d1: number, r0: number, r1: number): (v: number) => number {
  const k = d1 === d0 ? 0 : (r1 - r0) / (d1 - d0);
  return (v) => r0 + (v - d0) * k;
}

export function formatNumber(v: number | null | undefined, decimals?: number): string {
  if (v == null || !Number.isFinite(v)) return "na";
  const d = decimals ?? (Math.abs(v) >= 1000 || Number.isInteger(v) ? 0 : 1);
  return v.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

/** Short axis label: 12,000,000 → "12M", 4,000 → "4,000". */
export function axisLabel(v: number): string {
  if (Math.abs(v) >= 1_000_000) return `${Number((v / 1_000_000).toFixed(1))}M`;
  return v.toLocaleString("en-US");
}

/** The largest non-null value in some series. */
export function seriesMax(series: (number | null)[][]): number {
  let m = 0;
  for (const s of series) for (const v of s) if (v != null && v > m) m = v;
  return m;
}

/** "Percantage Urban" (the dataset's spelling) as the chart shows it. */
export function measureLabel(label: string): string {
  return label.replace(/^Percantage/, "Percentage");
}
