/**
 * Where `/item/{id}` sends items that have a page of their own elsewhere in the app: the 27
 * feature records (DATA `group="feat"`, `type` = `item.subtype`), topic timelines, panoramas
 * that only their Flash viewer shows, Map Trek stops. Pure, so the rules are unit-tested.
 */
import type { Item } from "../api/types";

/** Bilingual dictionary of a "Translate a Word" feature (`B<from>2<to>`; A = English). */
const LANG: Record<string, string> = { A: "ENG", E: "SPA", F: "FRA", D: "GER", I: "ITA" };

export function translatePair(subtype: string): string | null {
  const m = /^B([AEFDI])2([AEFDI])$/.exec(subtype);
  if (!m || m[1] === m[2]) return null;
  return `${LANG[m[1]!]}_${LANG[m[2]!]}`;
}

/** Features that were web services in 2009 (College Prep Online, Encarta on the Web). */
export const ONLINE_FEATURES = new Set(["Prep", "EONL"]);

/** The client route of a feature record, or null for the online-only ones. */
export function featureHref(subtype: string | undefined): string | null {
  const t = subtype ?? "";
  const pair = translatePair(t);
  if (pair) return `/translate?pair=${pair}`;
  switch (t) {
    case "DICT":
    case "QDCT":
      return "/dictionary";
    case "THES":
    case "QTHE":
      return "/thesaurus";
    case "BX2X":
      return "/translate";
    case "Time":
      return "/timeline";
    case "GeoQ":
      return "/games/geoquiz";
    case "Kids":
      return "/kids";
    case "QART":
      return "/search?kind=article";
    case "QMAP":
      return "/search?kind=map";
    case "QPHO":
      return "/media";
    case "QQUO":
      return "/search?kind=quotation";
    default:
      return null;
  }
}

/**
 * The TIMELINE.DAT term of a timeline-topic item: "Aztec Empire Topic Timeline" → "Aztec Empire",
 * "Timeline: 10th Century" → "10th Century"; other topics (event titles, search words) are terms
 * as they are.
 */
export function topicTerm(title: string): string {
  return title
    .replace(/\s+Topic Timeline$/i, "")
    .replace(/^Timeline:\s*/i, "")
    .trim();
}

/** The Timeline route of a timeline event (opened at the event) or topic (fitted to its events); null for other items. */
export function timelineHref(item: Pick<Item, "kind" | "ref_id" | "title" | "sort_title" | "timeline">): string | null {
  if (item.kind === "timeline-event") return item.timeline ? `/timeline?event=${item.ref_id}` : "/timeline";
  if (item.kind === "timeline-topic") {
    const term = topicTerm(item.sort_title || item.title || "");
    return term ? `/timeline?term=${encodeURIComponent(term)}` : "/timeline";
  }
  return null;
}

/** The Map Trek view of a stop's (or a tour's) Encarta atlas URL. */
export function mapTrekUrl(attrs: Record<string, unknown> | undefined): string | null {
  const lat = Number(attrs?.lat);
  const lon = Number(attrs?.long);
  if (!attrs || !Number.isFinite(lat) || !Number.isFinite(lon) || attrs.lat === undefined || attrs.long === undefined) return null;
  const alt = Number(attrs.alt);
  const style = Number(attrs.style);
  return `msencarta://view=MapView/cmd=map/lat=${lat}/long=${lon}${Number.isFinite(alt) && alt > 0 ? `/alt=${alt}` : ""}${Number.isFinite(style) ? `/style=${style}` : ""}`;
}

function hasPicture(item: Item): boolean {
  return (item.files ?? []).some((f) => f.role === "image" || f.role === "picon" || f.role === "default" || f.role === "surround");
}

/**
 * A route that replaces the item page, or null to show the item itself. Map Trek stops go to
 * their tour at that stop; the tour itself and tour nodes have views of their own.
 */
export function itemRedirect(item: Item): string | null {
  switch (item.kind) {
    case "feature": {
      return featureHref(item.subtype);
    }
    case "timeline-event": {
      // Like the VM, an event opens the Timeline view at the event, its pop-up open.
      return item.timeline ? timelineHref(item) : null;
    }
    case "timeline-topic":
      return timelineHref(item);
    case "panorama": {
      // The 151 panoramas carry only an icon: their pictures are in the Flash viewer that shows them.
      if (hasPicture(item)) return null;
      const viewer = (item.appears_in ?? []).find((a) => a.kind === "interactive");
      return viewer ? `/item/${viewer.ref_id}` : null;
    }
    case "maptrek-stop": {
      const tour = (item.appears_in ?? []).find((a) => a.kind === "maptrek-tour" || a.group === "maptrektour");
      return tour ? `/item/${tour.ref_id}?stop=${item.ref_id}` : null;
    }
    default:
      return null;
  }
}
