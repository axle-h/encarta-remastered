/**
 * Layout decisions for an article body, kept free of React so they can be unit-tested. The
 * converter stores the placement (content-schema §2): each `media` block's column (`text` or
 * `margin`), `cluster` blocks (several items in one margin box) and the Kids `media-grid`. The
 * reader only decides what goes to the right column (Quick Facts), into the text (inline tables)
 * or into a sidebar teaser.
 */
import type { AssocItem, DataTable } from "../api/types";
import type { Block, ClusterBlock, MediaBlock, MediaGridBlock, MediaRef } from "../content/model";

/**
 * Where a placed item is drawn: the left `margin` column (picon box), the `text` column (picture
 * at up to 432 px), `inline` (a table in the text), a sidebar `teaser`, the `right` column
 * (Quick Facts) or not at all.
 */
export type Placement = "margin" | "text" | "inline" | "teaser" | "right" | "hidden";

/** Width of a picture in the text column (px); wider ones are scaled down. */
export const WIDE_MAX_WIDTH = 432;

export function placementOf(b: MediaRef & { col?: string }, assoc: AssocItem | undefined, table: DataTable | undefined): Placement {
  if (assoc?.appears === 0) return "hidden";
  if (b.group === "tableinline") return "inline";
  if (table?.kind === "facts" && (b.group === "table" || b.group === "factsandfigures")) return "right";
  if (b.group === "sidebar") return "teaser";
  return b.col === "text" ? "text" : "margin";
}

/**
 * The members of a cluster that its box shows: every item except hidden ones. A facts table stays
 * a member (VM, Zebra: "Plains Zebra" and "Zebra Quick Facts" share a box) and is also in the right
 * column.
 */
export function clusterMembers(c: ClusterBlock, assoc: Map<number, AssocItem>): MediaRef[] {
  return c.items.filter((m) => assoc.get(m.ref)?.appears !== 0);
}

/** Every placed item of a body in reading order, cluster and grid members included. */
export function mediaInOrder(blocks: Block[], out: MediaRef[] = []): MediaRef[] {
  for (const b of blocks) {
    if (b.t === "media") out.push(b);
    else if (b.t === "cluster" || b.t === "media-grid") out.push(...b.items);
    else if (b.t === "section") mediaInOrder(b.blocks, out);
  }
  return out;
}

/** The Kids tile grid: the items of the body's `media-grid` blocks, in order. */
export function gridItems(blocks: Block[]): MediaRef[] {
  const out: MediaRef[] = [];
  const walk = (bs: Block[]) => {
    for (const b of bs) {
      if (b.t === "media-grid") out.push(...(b as MediaGridBlock).items);
      else if (b.t === "section") walk(b.blocks);
    }
  };
  walk(blocks);
  return out;
}

/**
 * A sidebar's body with its media first: the original floats all of a sidebar's own placed media
 * in one cluster at the top left of the body (content-schema §2 "Sidebar view").
 */
export function mediaFirst(blocks: Block[]): Block[] {
  const media = blocks.filter((b): b is ClusterBlock | MediaBlock => b.t === "cluster" || b.t === "media");
  return media.length ? [...media, ...blocks.filter((b) => b.t !== "cluster" && b.t !== "media")] : blocks;
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
