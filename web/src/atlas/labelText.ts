/**
 * Label text layouts as Encarta builds them (mad-styling.md §7.3 "Layouts and their cost"): where a
 * name breaks into lines, which layouts a label tries (with its alternative name, alone, abbreviated)
 * and what each layout costs. Pure; widths come from a measuring function.
 */

/** Characters after which a name may break: space, hyphen U+2010, en and em dash. Not "-". */
const BREAKS = new Set([0x20, 0x2010, 0x2013, 0x2014]);

/** Break positions of a text, and where a bracketed tail starts (0 = none). */
export function breakPoints(text: string): { breaks: number[]; bracket: number } {
  const breaks: number[] = [];
  let bracket = 0;
  for (let i = 0; i < text.length; i++) {
    if (BREAKS.has(text.charCodeAt(i))) breaks.push(i);
    if (i > 0 && text[i] === "(") {
      bracket = i;
      break;
    }
  }
  // The break before the bracket belongs to the tail, which is a line of its own.
  if (bracket && breaks.length && breaks[breaks.length - 1] !== 0) breaks.pop();
  return { breaks, bracket };
}

/** `text` on exactly `n` lines, or null when it cannot be split so (too few breaks, lines under 5 characters). */
export function splitLines(text: string, n: number): string[] | null {
  if (!text || n < 1) return null;
  const { breaks, bracket } = breakPoints(text);
  let head = text;
  let tail: string | null = null;
  if (n > 1 && bracket) {
    tail = text.slice(bracket);
    head = text.slice(0, text[bracket - 1] === " " ? bracket - 1 : bracket);
    n--;
  }
  if (breaks.length < n - 1) return null;
  const target = Math.floor(head.length / n);
  if (n > 1 && target < 5) return null;
  const lines: string[] = [];
  let start = 0;
  let k = 0;
  for (let line = 0; line < n - 1; line++) {
    // Keep one break for each line still to come.
    const last = breaks.length - (n - line - 1);
    while (k < last && breaks[k]! - start < target) {
      const next = breaks[k + 1]! - start;
      if (next > target && target - (breaks[k]! - start) < next - target) break;
      k++;
    }
    const b = breaks[k]!;
    lines.push(head.slice(start, text.charCodeAt(b) === 0x20 ? b : b + 1));
    start = b + 1;
    k++;
  }
  lines.push(head.slice(start));
  if (tail !== null) lines.push(tail);
  return lines;
}

/** Characters that get the label's letter spacing after them (all but the last, and not before "." or U+2019, nor after U+2018). */
export function spacedCount(line: string): number {
  let n = 0;
  for (let i = 0; i < line.length - 1; i++) {
    const next = line[i + 1];
    if (next === "." || next === "’" || line[i] === "‘") continue;
    n++;
  }
  return n;
}

/** Encarta's cost of a layout: about 12 characters per line is ideal, very short and long lines cost extra. */
export function layoutCost(lines: readonly string[]): number {
  const total = lines.reduce((s, l) => s + l.length, 0);
  const ideal = Math.ceil(total / 12);
  let cost = 250 * Math.abs(lines.length - ideal);
  const len = lines.map((l) => l.length);
  if (len.some((l) => l >= 3 && l <= 4)) cost += 250;
  if (len.some((l) => l < 3)) cost += 500;
  if (len.some((l) => l >= 13 && l <= 20)) cost += 250;
  if (len.some((l) => l >= 21)) cost += 500;
  return cost;
}

/** The three groups of layouts, tried in this order. */
export type LayoutGroup = "alt" | "main" | "abbr";

export interface Layout {
  group: LayoutGroup;
  /** The main name's (or abbreviation's) lines. */
  lines: string[];
  /** The alternative name, drawn as an extra last line in the alternative style. */
  alt?: string;
  cost: number;
}

/** The texts of a label (upper case already applied). */
export interface LabelTexts {
  main: string;
  alt?: string;
  abbr?: string;
}

/** Every layout of a label, grouped in the order Encarta tries them (empty groups left out). */
export function labelLayouts(t: LabelTexts): Layout[][] {
  const groups: Layout[][] = [];
  const build = (group: LayoutGroup, text: string, alt?: string) => {
    const out: Layout[] = [];
    for (let n = 1; n <= 3; n++) {
      const lines = splitLines(text, n);
      if (!lines) break;
      out.push({ group, lines, alt, cost: layoutCost(alt !== undefined ? [...lines, alt] : lines) });
    }
    if (out.length) groups.push(out);
  };
  if (t.alt && t.main) build("alt", t.main, t.alt);
  if (t.main) build("main", t.main);
  if (t.abbr) build("abbr", t.abbr);
  return groups;
}

/** Kana or a CJK ideograph first: the alternative goes in brackets and the lines are centred. */
export function isCjk(text: string): boolean {
  const c = text.codePointAt(0) ?? 0;
  return (c >= 0x3040 && c <= 0x30ff) || (c >= 0x3300 && c <= 0x9faf);
}
