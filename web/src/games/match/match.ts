/**
 * Match games (IAF `MatchGame` interactives, docs/games.md "Match games"): reading the `.iax`
 * definition into levels of items, and the scoring rules.
 */
import type { MediaInfo } from "../../api/types";

export interface Bucket {
  title: string;
  body: string;
  image?: string;
  audio?: string;
}

export interface MatchItem {
  id: string;
  top: Bucket;
  bottom: Bucket;
  solved: Bucket;
}

export interface MatchLevel {
  caption: string;
  title: string;
  /** How many items are on the board at once. */
  perBoard: number;
  topType: "image" | "text";
  bottomType: "image" | "text";
  items: MatchItem[];
  rewards: string[];
}

export interface MatchDefinition {
  levels: MatchLevel[];
  strings: { level: string; score: string; matchesIn: string; tries: string; reset: string };
  credits: string;
}

type Obj = Record<string, unknown>;

function list(v: unknown): Obj[] {
  if (Array.isArray(v)) return v as Obj[];
  if (v && typeof v === "object") return [v as Obj];
  return [];
}

function str(v: unknown): string {
  if (typeof v === "string") return v.trim();
  if (v && typeof v === "object" && typeof (v as Obj).value === "string") return ((v as Obj).value as string).trim();
  return "";
}

function bucket(v: unknown): Bucket {
  const o = (v ?? {}) as Obj;
  const cap = (o.caption ?? {}) as Obj;
  return { title: str(cap.title), body: str(cap.body), image: str(o.image) || str(cap.image) || undefined, audio: str(o.audiofile) || undefined };
}

export function parseMatch(def: unknown): MatchDefinition {
  const d = (def ?? {}) as Obj;
  const slides = list((d.slides as Obj | undefined)?.slide).sort((a, b) => Number(a["@sequence"] ?? 0) - Number(b["@sequence"] ?? 0));
  const levels: MatchLevel[] = slides.map((s) => {
    const ss = (s.standardslide ?? {}) as Obj;
    const cap = (ss.caption ?? {}) as Obj;
    const items = list((ss.items as Obj | undefined)?.item).map((it, i) => ({
      id: String(it["@id"] ?? i),
      top: bucket(it.topbucket),
      bottom: bucket(it.bottombucket),
      solved: bucket(it.solvedbucket),
    }));
    return {
      caption: str(cap.body),
      title: str(cap.title),
      perBoard: Math.max(2, Number(s["@itemsperbucket"]) || 8),
      topType: String(s["@topbuckettype"] ?? "image").toLowerCase() === "text" ? "text" : "image",
      bottomType: String(s["@bottombuckettype"] ?? "text").toLowerCase() === "image" ? "image" : "text",
      items,
      rewards: list((ss.rewards as Obj | undefined)?.reward).map((r) => str(r.text)),
    };
  });
  const st = (((d.iaigame as Obj | undefined)?.strings ?? {}) as Obj);
  return {
    levels,
    strings: {
      level: str(st.currentleveltext) || "Level:",
      score: str(st.scorelabeltext) || "SCORE:",
      matchesIn: str(st.matcheslabeltext) || "matches in",
      tries: str(st.trieslabeltext) || "tries",
      reset: str(st.resetbuttontext) || "Play Again!",
    },
    credits: str((d.creditsdialogdata as Obj | undefined)?.text),
  };
}

/** Points per match (our choice; the definition does not carry one). */
export const MATCH_POINTS = 10;

/** The reward line: perfect (no misses), then the second and third texts by miss rate. */
export function rewardIndex(matches: number, tries: number): number {
  const misses = Math.max(0, tries - matches);
  if (misses === 0) return 0;
  return misses <= matches / 2 ? 1 : 2;
}

/** A media reference of the definition → a URL, through the interactive's resolved media map. */
export function mediaUrl(media: Record<string, MediaInfo>, name: string | undefined, kind: "image" | "audio"): string | undefined {
  if (!name) return undefined;
  const info = media[name] ?? Object.entries(media).find(([k]) => k.toLowerCase() === name.toLowerCase())?.[1];
  const v = info?.variants ?? {};
  if (kind === "image") return v.webp?.url ?? v.original?.url ?? `/media/file/${name.toLowerCase()}`;
  return v.opus?.url ?? v.aac?.url ?? v.mp3?.url ?? v.original?.url;
}
