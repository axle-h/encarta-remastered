/**
 * Choosing what to show or play for an item from its DATA files (api.md `files[]`, roles from
 * content-schema §3) and the media manifest's variants (media.md §2).
 */
import type { FileRef, Item, ItemFile, MediaInfo } from "../api/types";

export function fileByRole(item: Pick<Item, "files"> | undefined, ...roles: string[]): ItemFile | undefined {
  if (!item?.files) return undefined;
  for (const r of roles) {
    const f = item.files.find((x) => x.role === r);
    if (f) return f;
  }
  return undefined;
}

/** URL of a disc file by name (server content-negotiates images). */
export function fileUrl(f: FileRef | undefined | null): string | undefined {
  if (!f) return undefined;
  return f.url || `/media/file/${f.file_name}`;
}

/** The largest still picture of an item: the full image, else the gallery picture, else the thumbnail. */
export function largeImage(item: Item | undefined): ItemFile | undefined {
  return fileByRole(item, "image", "picon", "thumb");
}

/** The gallery-size picture (what the article's media column shows). */
export function galleryImage(item: Item | undefined): ItemFile | undefined {
  return fileByRole(item, "picon", "image", "thumb");
}

export interface Source {
  src: string;
  type: string;
}

const AUDIO_ORDER: [string, string][] = [
  ["opus", "audio/ogg; codecs=opus"],
  ["aac", "audio/mp4"],
  ["mp3", "audio/mpeg"],
];
const VIDEO_ORDER: [string, string][] = [
  ["webm", "video/webm"],
  ["mp4", "video/mp4"],
];

function sourcesOf(media: MediaInfo | null | undefined, order: [string, string][], fallback?: ItemFile): Source[] {
  const out: Source[] = [];
  const v = media?.variants ?? {};
  for (const [name, type] of order) {
    const x = v[name];
    if (x) out.push({ src: x.url, type: x.mime ?? type });
  }
  // MP3 originals play as they are (media.md §2: kept, no derivatives).
  const orig = v.original;
  if (orig && (media?.format === "mp3" || orig.mime === "audio/mpeg")) out.push({ src: orig.url, type: "audio/mpeg" });
  if (out.length === 0 && fallback && /\.(mp3|ogg|m4a|mp4|webm)$/i.test(fallback.file_name)) {
    out.push({ src: fileUrl(fallback)!, type: "" });
  }
  return out;
}

export function audioSources(f: ItemFile | undefined): Source[] {
  return f ? sourcesOf(f.media, AUDIO_ORDER, f) : [];
}

/** True when the audio file is MIDI (played by our synth, media.md §2: kept, no derivatives). */
export function isMidi(f: ItemFile | undefined): boolean {
  if (!f) return false;
  return f.media?.format === "midi" || /\.midi?$/i.test(f.file_name);
}

/** URL of a file's original bytes. */
export function originalUrl(f: ItemFile): string {
  return f.media?.variants.original?.url ?? `${fileUrl(f)}?original=1`;
}

/** Whether the item has something to play: a converted audio source or a MIDI file. */
export function hasAudio(item: Item | undefined): boolean {
  const f = fileByRole(item, "audio");
  return isMidi(f) || audioSources(f).length > 0;
}

export function videoSources(f: ItemFile | undefined): Source[] {
  return f ? sourcesOf(f.media, VIDEO_ORDER, f) : [];
}

export function posterUrl(item: Item | undefined): string | undefined {
  const v = fileByRole(item, "video");
  const p = v?.media?.variants.poster;
  return p?.url ?? fileUrl(galleryImage(item));
}

/** WebVTT caption track URLs from the `cc` file(s) (SAMI converted, media.md §2.5). */
export function captionTracks(item: Item | undefined): { src: string; label: string; lang: string }[] {
  const out: { src: string; label: string; lang: string }[] = [];
  for (const f of item?.files ?? []) {
    if (f.role !== "cc" || !f.media) continue;
    for (const [name, v] of Object.entries(f.media.variants)) {
      if (name === "vtt" || name.startsWith("vtt-")) out.push({ src: v.url, label: "English captions", lang: "en" });
    }
  }
  return out;
}

export type MediaClass = "image" | "audio" | "video" | "table" | "interactive" | "tour" | "map" | "text" | "other";

/** What kind of viewer an item needs. */
export function mediaClass(item: { kind?: string; media_type?: string; group?: string } | undefined): MediaClass {
  if (!item) return "other";
  const k = item.kind ?? "";
  const t = item.media_type ?? "";
  // The kind wins over the icon: 57 narrated slide shows (`narr`) and 25 Flash maps (`map`) are
  // interactives with an `.iax`, not audio or atlas maps.
  if (k === "interactive") return "interactive";
  if (k === "video" || t === "video") return "video";
  if (k === "audio" || k === "audio-set" || t === "audio" || t === "narr") return "audio";
  if (k === "table" || k === "facts" || t === "table" || t === "chart") return "table";
  if (k === "virtual-tour" || k === "virtual-tour-node" || t === "vtour") return "tour";
  if (k === "interactive" || t === "ia" || t === "game") return "interactive";
  if (k === "map" || t === "map") return "map";
  if (k === "sidebar" || k === "article" || k === "further-reading" || t === "sidebar" || t === "quo") return "text";
  if (k === "media" || k === "panorama" || k === "composite-media" || t === "image" || t === "pan") return "image";
  return "other";
}

/** The 3D tour id (`out/tours/<id>`) of a tour content id, given the tours index. */
export function tourIdFor(contentId: number, tours: { id: string; contentId: number }[] | undefined): string | undefined {
  return tours?.find((t) => t.contentId === contentId)?.id;
}

/** Associations without repeats of the same item (the server lists one row per association). */
export function uniqueByRef<T extends { ref_id: number }>(list: T[]): T[] {
  const seen = new Set<number>();
  return list.filter((x) => (seen.has(x.ref_id) ? false : (seen.add(x.ref_id), true)));
}
