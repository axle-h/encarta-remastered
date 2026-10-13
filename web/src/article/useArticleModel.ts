import { useMemo } from "react";
import type { Article, AssocItem, DataTable } from "../api/types";
import { walkSections, type Block } from "../content/model";
import { mediaInOrder, mediaSequence, placementOf } from "./layout";

/** Lookup tables and derived lists for rendering one article. */
export function useArticleModel(article: Article | undefined) {
  return useMemo(() => {
    const body: Block[] = article?.body ?? [];
    const media = article?.media ?? [];
    const assoc = new Map<number, AssocItem>(media.map((m) => [m.ref_id, m]));
    const tables = new Map<number, DataTable>();
    for (const t of article?.tables ?? []) if (t.ref_id !== undefined) tables.set(t.ref_id, t);
    const sequence = mediaSequence(body, media);
    // Facts tables placed in the body (or associated) go to the right column.
    const right: (DataTable & { ref_id: number })[] = [];
    const seen = new Set<number>();
    for (const b of mediaInOrder(body)) {
      const t = tables.get(b.ref);
      if (t && placementOf(b, assoc.get(b.ref), t) === "right" && !seen.has(b.ref)) {
        seen.add(b.ref);
        right.push({ ...t, ref_id: b.ref });
      }
    }
    const sections = walkSections(body);
    return { body, media, assoc, tables, sequence, right, sections };
  }, [article]);
}
