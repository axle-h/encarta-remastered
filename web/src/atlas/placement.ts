/**
 * Encarta's label placement (mad-styling.md §7.4, §8.1): one list of every label of the view in
 * priority order; each label takes its cheapest candidate box, where a box outside the pane or over a
 * placed label is rejected and the others cost a position step, what lies under them (water, land,
 * lines), the boxes that labelled points reserve around themselves and their text layout. Pure: the
 * map glue (labelOverlay.ts) supplies screen positions, text sizes and the pixel classes.
 */
import type { Layout } from "./labelText";

export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function intersects(a: Box, b: Box): boolean {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
}

/** A layout with its measured box. */
export interface SizedLayout extends Layout {
  w: number;
  h: number;
}

/** What sorts the label list (§7.4 "Placement order"); smaller first. */
export interface LabelOrder {
  flagged: boolean;
  priority: number;
  icon: number;
  geoobj: number;
  entity: number;
}

export function compareOrder(a: LabelOrder, b: LabelOrder): number {
  if (a.flagged !== b.flagged) return a.flagged ? -1 : 1;
  return a.priority - b.priority || a.icon - b.icon || a.geoobj - b.geoobj || a.entity - b.entity;
}

/** A point's or an area's label. */
export interface TextLabel {
  kind: "point" | "area";
  x: number;
  y: number;
  /** Symbol size in px (0: no symbol). */
  symbol: number;
  /** Layout groups in the order they are tried (labelText.ts); empty: a symbol without a name. */
  groups: SizedLayout[][];
  /** Areas: layouts wider than this are rejected, except in the last group. */
  maxWidth?: number;
  /** A labelled point's reserved neighbour box: four "X" widths by twice the symbol size. */
  reserve?: { w: number; h: number };
  order: LabelOrder;
}

/** A road run with a shield label (§8.1), in screen px. */
export interface ShieldRun {
  kind: "shield";
  line: [number, number][];
  w: number;
  h: number;
  text: string;
  order: LabelOrder;
}

export type Label = TextLabel | ShieldRun;

/** Candidate positions of a point label, in Encarta's order with their added costs. */
export type Position = "center" | "above-right" | "below-right" | "above-left" | "below-left";
export const POSITIONS: readonly { pos: Position; step: number }[] = [
  { pos: "center", step: 0 },
  { pos: "above-right", step: 250 },
  { pos: "below-right", step: 500 },
  { pos: "above-left", step: 750 },
  { pos: "below-left", step: 1000 },
];

/** The text box of a candidate: `r` = half the symbol size (§7.4 table). */
export function candidateBox(pos: Position, x: number, y: number, r: number, w: number, h: number): Box {
  switch (pos) {
    case "center":
      return { x0: x - w / 2, y0: y - h / 2, x1: x + w / 2, y1: y + h / 2 };
    case "above-right":
      return { x0: x + r + 1, y0: y - h, x1: x + r + 1 + w, y1: y };
    case "below-right":
      return { x0: x + r + 1, y0: y + r + 1, x1: x + r + 1 + w, y1: y + r + 1 + h };
    case "above-left":
      return { x0: x - r - 1 - w, y0: y - h, x1: x - r - 1, y1: y };
    case "below-left":
      return { x0: x - r - w, y0: y + r, x1: x - r, y1: y + r + h };
  }
}

/** Pixel counts under a box by class group (§5.2 classes). */
export interface ClassCounts {
  water: number;
  land: number;
  /** Classes 0, 6 and 8: unassigned, label colours and line pens with `b14 = 3`. */
  marked: number;
  total: number;
}

/** Background cost of a box from its pixel classes (§7.4). */
export function backgroundCost(c: ClassCounts): number {
  if (c.total <= 0) return 0;
  const water = c.water / c.total;
  const land = c.land / c.total;
  let cost: number;
  if (water > 0.97) cost = 0;
  else if (land > 0.97 || water >= 0.9) cost = 251;
  else if (water <= 0.1 || water >= 0.5) cost = 1000;
  else cost = 2000;
  const marked = c.marked / c.total;
  if (marked > 0.1) cost += 3000;
  else if (marked > 0.01) cost += 2000;
  else if (c.marked > 0) cost += 1000;
  return cost;
}

/** Boxes in a uniform grid, for overlap queries. */
export class BoxIndex<T extends { box: Box }> {
  private cells = new Map<number, T[]>();
  constructor(private size = 64) {}
  private *keys(b: Box): Generator<number> {
    const s = this.size;
    for (let gx = Math.floor(b.x0 / s); gx <= Math.floor((b.x1 - 1e-6) / s); gx++)
      for (let gy = Math.floor(b.y0 / s); gy <= Math.floor((b.y1 - 1e-6) / s); gy++) yield (gx + 4096) * 8192 + (gy + 4096);
  }
  add(item: T): void {
    for (const k of this.keys(item.box)) {
      const c = this.cells.get(k);
      if (c) c.push(item);
      else this.cells.set(k, [item]);
    }
  }
  /** Every item whose box intersects `b`, once. */
  query(b: Box): T[] {
    const seen = new Set<T>();
    for (const k of this.keys(b)) for (const it of this.cells.get(k) ?? []) if (!seen.has(it) && intersects(it.box, b)) seen.add(it);
    return [...seen];
  }
  any(b: Box): boolean {
    for (const k of this.keys(b)) for (const it of this.cells.get(k) ?? []) if (intersects(it.box, b)) return true;
    return false;
  }
}

export interface PlaceEnv {
  /** Pane size in px. */
  width: number;
  height: number;
  /** A box wholly inside the pane (and, on the globe, on the disc). */
  inside: (b: Box) => boolean;
  /** Pixel classes under a box; omitted: no background cost. */
  classes?: (b: Box) => ClassCounts;
}

export interface TextPlacement {
  kind: "point" | "area";
  label: TextLabel;
  /** The layout and position taken; undefined for a symbol drawn without its name. */
  layout?: SizedLayout;
  pos?: Position;
  box?: Box;
  /** The symbol is drawn (false: it overlapped a placed label, so the name went alone). */
  icon: boolean;
}

export interface ShieldPlacement {
  kind: "shield";
  label: ShieldRun;
  x: number;
  y: number;
  box: Box;
}

export type Placement = TextPlacement | ShieldPlacement;

interface Reserved {
  box: Box;
  weight: number;
  owner: Label;
}

/** The neighbour boxes of §7.4: labelled points by priority group, plus the pane centre. */
function reservedBoxes(sorted: readonly Label[], env: PlaceEnv): BoxIndex<Reserved> {
  const index = new BoxIndex<Reserved>();
  const cx = env.width / 2;
  const cy = env.height / 2;
  index.add({ box: { x0: cx - 10, y0: cy - 10, x1: cx + 10, y1: cy + 10 }, weight: 5000, owner: sorted[0]! });
  let weight = 5000;
  let group: number | null = null;
  for (const l of sorted) {
    if (l.kind !== "point" || !l.reserve || !l.groups.length) continue;
    if (l.order.priority !== group) {
      group = l.order.priority;
      weight = Math.max(251, weight - 250);
    }
    const { w, h } = l.reserve;
    index.add({ box: { x0: l.x - w / 2, y0: l.y - h / 2, x1: l.x + w / 2, y1: l.y + h / 2 }, weight, owner: l });
  }
  return index;
}

/** Places every label in Encarta's order. Returns the labels that are drawn, in that order. */
export function placeLabels(labels: readonly Label[], env: PlaceEnv): Placement[] {
  const sorted = [...labels].sort((a, b) => compareOrder(a.order, b.order));
  const reserved = sorted.length ? reservedBoxes(sorted, env) : new BoxIndex<Reserved>();
  const placed = new BoxIndex<{ box: Box }>();
  const shieldsByText = new Map<string, [number, number][]>();
  const near = Math.hypot(env.width / 6, env.height / 6);
  const out: Placement[] = [];
  const neighbourCost = (b: Box, self: Label) => {
    let c = 0;
    for (const r of reserved.query(b)) if (r.owner !== self || r.weight === 5000) c += r.weight;
    return c;
  };
  const free = (b: Box) => env.inside(b) && !placed.any(b);

  for (const l of sorted) {
    if (l.kind === "shield") {
      const p = placeShield(l, free, (b) => neighbourCost(b, l));
      if (!p) continue;
      const same = shieldsByText.get(l.text) ?? [];
      if (same.some(([x, y]) => Math.hypot(x - p.x, y - p.y) <= near)) continue;
      same.push([p.x, p.y]);
      shieldsByText.set(l.text, same);
      placed.add({ box: p.box });
      out.push({ kind: "shield", label: l, ...p });
      continue;
    }
    // A symbol over a placed label is not drawn; its name is then placed as if it had none (mad-styling.md §7.4 "The symbol").
    let icon = l.symbol > 0;
    const symbolBox: Box = { x0: l.x - l.symbol / 2, y0: l.y - l.symbol / 2, x1: l.x + l.symbol / 2, y1: l.y + l.symbol / 2 };
    if (icon && !free(symbolBox)) icon = false;
    if (!l.groups.length) {
      if (!icon) continue;
      out.push({ kind: l.kind, label: l, icon: true });
      placed.add({ box: inflate(symbolBox, l.symbol / 4) });
      continue;
    }
    const r = icon ? Math.floor(l.symbol / 2) : 0;
    let best: { cost: number; layout: SizedLayout; pos: Position; box: Box } | null = null;
    for (let g = 0; g < l.groups.length && !best; g++) {
      const lastGroup = g === l.groups.length - 1;
      for (const layout of l.groups[g]!) {
        if (l.maxWidth !== undefined && !lastGroup && layout.w > l.maxWidth) continue;
        for (const { pos, step } of POSITIONS) {
          // Areas: centred only. Points: centred only without a (drawn) symbol.
          if (l.kind === "area" ? pos !== "center" : pos === "center" && r !== 0) continue;
          const box = candidateBox(pos, l.x, l.y, r, layout.w, layout.h);
          if (!free(box)) continue;
          const cost = step + layout.cost + (env.classes ? backgroundCost(env.classes(box)) : 0) + neighbourCost(box, l);
          if (!best || cost < best.cost) best = { cost, layout, pos, box };
        }
      }
    }
    if (!best) continue; // neither the name nor (for a point) its symbol is drawn
    placed.add({ box: best.box });
    if (icon) placed.add({ box: inflate(symbolBox, l.symbol / 4) });
    out.push({ kind: l.kind, label: l, layout: best.layout, pos: best.pos, box: best.box, icon });
  }
  return out;
}

function inflate(b: Box, d: number): Box {
  return { x0: b.x0 - d, y0: b.y0 - d, x1: b.x1 + d, y1: b.y1 + d };
}

/** Length of a polyline and the point at distance `d` along it. */
export function pointAlong(line: readonly [number, number][], d: number): [number, number] {
  let rest = d;
  for (let i = 1; i < line.length; i++) {
    const [ax, ay] = line[i - 1]!;
    const [bx, by] = line[i]!;
    const s = Math.hypot(bx - ax, by - ay);
    if (rest <= s || i === line.length - 1) {
      const t = s > 0 ? Math.min(1, rest / s) : 0;
      return [ax + (bx - ax) * t, ay + (by - ay) * t];
    }
    rest -= s;
  }
  return line[0] ?? [0, 0];
}

export function lineLength(line: readonly [number, number][]): number {
  let L = 0;
  for (let i = 1; i < line.length; i++) L += Math.hypot(line[i]![0] - line[i - 1]![0], line[i]![1] - line[i - 1]![1]);
  return L;
}

/**
 * The shield of one run (§8.1): up to 16 evenly spaced candidates from one shield width after the start
 * to one before the end; the cheapest free one by distance from the middle plus the neighbour cost.
 */
export function placeShield(run: ShieldRun, free: (b: Box) => boolean, neighbours: (b: Box) => number): { x: number; y: number; box: Box } | null {
  const L = lineLength(run.line);
  const sw = run.w;
  if (L <= 2 * sw) return null;
  const n = Math.min(16, Math.floor(L / 10));
  if (n < 1) return null;
  const half = L / 2;
  let best: { cost: number; x: number; y: number; box: Box } | null = null;
  for (let i = 0; i < n; i++) {
    const d = n === 1 ? half : sw + ((L - 2 * sw) * i) / (n - 1);
    const [x, y] = pointAlong(run.line, d);
    const box = { x0: x - run.w / 2, y0: y - run.h / 2, x1: x + run.w / 2, y1: y + run.h / 2 };
    if (!free(box)) continue;
    const cost = (4000 * Math.abs(d - half)) / half + neighbours(box);
    if (!best || cost < best.cost) best = { cost, x, y, box };
  }
  return best && { x: best.x, y: best.y, box: best.box };
}

/**
 * Joins the pieces of one road (tile pieces, clipped to the pane) into on-screen runs: pieces whose
 * ends meet within `tol` px are chained, reversing them as needed.
 */
export function joinRuns(pieces: readonly [number, number][][], tol = 1.5): [number, number][][] {
  const left = pieces.filter((p) => p.length >= 2).map((p) => [...p]);
  const runs: [number, number][][] = [];
  const close = (a: [number, number], b: [number, number]) => Math.abs(a[0] - b[0]) <= tol && Math.abs(a[1] - b[1]) <= tol;
  while (left.length) {
    let run = left.pop()!;
    let grew = true;
    while (grew) {
      grew = false;
      for (let i = 0; i < left.length; i++) {
        const p = left[i]!;
        const head = run[0]!;
        const tail = run[run.length - 1]!;
        if (close(tail, p[0]!)) run = run.concat(p.slice(1));
        else if (close(tail, p[p.length - 1]!)) run = run.concat(p.slice(0, -1).reverse());
        else if (close(head, p[p.length - 1]!)) run = p.slice(0, -1).concat(run);
        else if (close(head, p[0]!)) run = p.slice(1).reverse().concat(run);
        else continue;
        left.splice(i, 1);
        grew = true;
        break;
      }
    }
    runs.push(run);
  }
  return runs;
}

/** Clips a polyline to a box; returns the pieces inside (Liang–Barsky per segment). */
export function clipLine(line: readonly [number, number][], b: Box): [number, number][][] {
  const out: [number, number][][] = [];
  let cur: [number, number][] | null = null;
  for (let i = 1; i < line.length; i++) {
    const seg = clipSegment(line[i - 1]!, line[i]!, b);
    if (!seg) {
      cur = null;
      continue;
    }
    const [p, q, clippedEnd] = seg;
    if (!cur || cur[cur.length - 1]![0] !== p[0] || cur[cur.length - 1]![1] !== p[1]) {
      cur = [p];
      out.push(cur);
    }
    cur.push(q);
    if (clippedEnd) cur = null;
  }
  return out;
}

function clipSegment(a: [number, number], c: [number, number], b: Box): [[number, number], [number, number], boolean] | null {
  let t0 = 0;
  let t1 = 1;
  const dx = c[0] - a[0];
  const dy = c[1] - a[1];
  const edges: [number, number][] = [
    [-dx, a[0] - b.x0],
    [dx, b.x1 - a[0]],
    [-dy, a[1] - b.y0],
    [dy, b.y1 - a[1]],
  ];
  for (const [p, q] of edges) {
    if (p === 0) {
      if (q < 0) return null;
      continue;
    }
    const r = q / p;
    if (p < 0) {
      if (r > t1) return null;
      if (r > t0) t0 = r;
    } else {
      if (r < t0) return null;
      if (r < t1) t1 = r;
    }
  }
  return [
    [a[0] + dx * t0, a[1] + dy * t0],
    [a[0] + dx * t1, a[1] + dy * t1],
    t1 < 1,
  ];
}
