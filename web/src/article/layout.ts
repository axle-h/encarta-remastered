/**
 * Layout decisions for an article body, kept free of React so they can be unit-tested:
 * which placed items go to the left media column, inline into the text, or to the right column,
 * and how consecutive media blocks group into one box (the original shows them as one box
 * with a row of small thumbnails to switch between them).
 */
import type { AssocItem, DataTable } from "../api/types";
import type { Block, MediaBlock } from "../content/model";

export type Placement = "left" | "inline" | "right" | "hidden";

export function placementOf(b: MediaBlock, assoc: AssocItem | undefined, table: DataTable | undefined): Placement {
  if (assoc?.appears === 0) return "hidden";
  if (b.group === "tableinline") return "inline";
  if (table?.kind === "facts" && (b.group === "table" || b.group === "factsandfigures")) return "right";
  return "left";
}

export type Run =
  | { t: "media"; placement: Placement; items: MediaBlock[] }
  | { t: "block"; block: Block };

/** Splits a section's blocks into runs, grouping consecutive media blocks with the same placement. */
export function runsOf(
  blocks: Block[],
  assoc: Map<number, AssocItem>,
  tables: Map<number, DataTable>,
): Run[] {
  const out: Run[] = [];
  for (const b of blocks) {
    if (b.t !== "media") {
      out.push({ t: "block", block: b });
      continue;
    }
    const placement = placementOf(b, assoc.get(b.ref), tables.get(b.ref));
    const last = out[out.length - 1];
    // Inline tables each stand alone; left-column items group until the next text block.
    if (last && last.t === "media" && last.placement === placement && placement !== "inline") last.items.push(b);
    else out.push({ t: "media", placement, items: [b] });
  }
  return out;
}

/** All media blocks of a body in reading order (the strip under the title, the viewer sequence). */
export function mediaInOrder(blocks: Block[], out: MediaBlock[] = []): MediaBlock[] {
  for (const b of blocks) {
    if (b.t === "media") out.push(b);
    else if (b.t === "section") mediaInOrder(b.blocks, out);
  }
  return out;
}

/** Groups that the strip and the "All Media" gallery show (not further reading, contributors …). */
export const SHOWN_GROUPS = new Set([
  "media",
  "audio",
  "audionested",
  "video",
  "flash",
  "table",
  "tableinline",
  "sidebar",
  "composite",
  "virttour",
  "factsandfigures",
  "timeevent",
  "timetopic",
  "maps",
]);

/**
 * The article's media sequence: placed body media first in reading order, then any other shown
 * association not placed in the body, without duplicates.
 */
export function mediaSequence(blocks: Block[], media: AssocItem[]): AssocItem[] {
  const byRef = new Map(media.map((m) => [m.ref_id, m]));
  const seen = new Set<number>();
  const out: AssocItem[] = [];
  for (const b of mediaInOrder(blocks)) {
    if (seen.has(b.ref)) continue;
    seen.add(b.ref);
    out.push(byRef.get(b.ref) ?? { ref_id: b.ref, group: b.group });
  }
  for (const m of media) {
    if (seen.has(m.ref_id) || !SHOWN_GROUPS.has(m.group) || m.group === "maps" || m.appears === 0) continue;
    seen.add(m.ref_id);
    out.push(m);
  }
  return out;
}

/** The first paragraph of a body (for the outline view's preview). */
export function firstParagraph(blocks: Block[]): Block | undefined {
  for (const b of blocks) {
    if (b.t === "p" || b.t === "intro") return b;
    if (b.t === "section") {
      const p = firstParagraph(b.blocks);
      if (p) return p;
    }
  }
  return undefined;
}

/** Id of the section that contains paragraph `para` (for deep links into collapsed sections). */
export function sectionPathOf(blocks: Block[], para: number, path: number[] = []): number[] | null {
  for (const b of blocks) {
    if ((b.t === "p" || b.t === "intro" || b.t === "byline" || b.t === "heading") && b.id === para) return path;
    if (b.t === "section") {
      const r = sectionPathOf(b.blocks, para, [...path, b.id]);
      if (r) return r;
    }
  }
  return null;
}
