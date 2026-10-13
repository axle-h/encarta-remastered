/**
 * Unit tests for the web client's pure logic, on synthetic data only (CLEAN_ROOM rule 5).
 */
import { describe, expect, it } from "vitest";
import { query } from "./api/client";
import type { AssocItem, DataTable, Item, TimelineEvent } from "./api/types";
import { firstParagraph, mediaInOrder, mediaSequence, placementOf, runsOf, sectionPathOf } from "./article/layout";
import { quickEntries } from "./article/SidePanels";
import { appUrlHref, itemHref, rewriteHref } from "./content/links";
import { linkPrefix, linkSuffix, plainText, walkSections, type Block } from "./content/model";
import { parseVtt } from "./media/MidiPlayer";
import { midiFrequency, parseMidi } from "./media/midi";
import { audioSources, captionTracks, fileByRole, hasAudio, mediaClass, tourIdFor, uniqueByRef } from "./media/pick";
import { GRAMMAR_CODES } from "./pages/dictCodes";
import { reversePair } from "./pages/DictionaryPage";
import { homeLinkHref, homeSections, themeIcons } from "./pages/home";
import { formatDate, formatEventDate, yearLabel } from "./pages/timelineDates";
import { categoryIndex, pan, placeEvents, ticks, tickStep, zoom } from "./pages/timelineLayout";
import { kidsUrl } from "./components/SearchBox";
import { atlasInitialPlace } from "./app/App";

const body: Block[] = [
  {
    t: "section",
    id: 1,
    kind: "introduction",
    level: 1,
    number: ["I"],
    title: ["INTRODUCTION"],
    blocks: [
      { t: "media", ref: 100, group: "media", seq: 1 },
      { t: "media", ref: 101, group: "audionested", seq: 2 },
      { t: "p", id: 1, c: ["First ", { t: "em", c: ["para"] }] },
      { t: "media", ref: 102, group: "table", seq: 3 },
      { t: "p", id: 2, c: ["Second"] },
    ],
  },
  {
    t: "section",
    id: 2,
    kind: "major",
    level: 1,
    number: ["II"],
    title: ["MORE"],
    blocks: [
      { t: "section", id: 3, kind: "minor", level: 2, number: ["II", "A"], title: ["Sub"], blocks: [{ t: "p", id: 7, c: ["Deep"] }] },
      { t: "media", ref: 103, group: "tableinline" },
    ],
  },
];

const facts: DataTable = { ref_id: 102, kind: "facts", json: { entries: [] }, html: "" };
const inline: DataTable = { ref_id: 103, kind: "table", json: {}, html: "<table></table>" };

describe("article layout", () => {
  const assoc = new Map<number, AssocItem>([[100, { ref_id: 100, group: "media" }]]);
  const tables = new Map<number, DataTable>([
    [102, facts],
    [103, inline],
  ]);

  it("places facts tables right, inline tables inline, the rest left", () => {
    expect(placementOf({ t: "media", ref: 102, group: "table" }, undefined, facts)).toBe("right");
    expect(placementOf({ t: "media", ref: 103, group: "tableinline" }, undefined, inline)).toBe("inline");
    expect(placementOf({ t: "media", ref: 100, group: "media" }, undefined, undefined)).toBe("left");
    expect(placementOf({ t: "media", ref: 9, group: "table" }, undefined, inline)).toBe("left");
  });

  it("groups consecutive media blocks into one box", () => {
    const s = body[0] as Extract<Block, { t: "section" }>;
    const runs = runsOf(s.blocks, assoc, tables);
    expect(runs.map((r) => (r.t === "media" ? `${r.placement}:${r.items.map((i) => i.ref).join("+")}` : r.block.t))).toEqual([
      "left:100+101",
      "p",
      "right:102",
      "p",
    ]);
  });

  it("orders the media sequence by reading order, then other shown associations", () => {
    expect(mediaInOrder(body).map((m) => m.ref)).toEqual([100, 101, 102, 103]);
    const media: AssocItem[] = [
      { ref_id: 104, group: "video" },
      { ref_id: 101, group: "audionested", title: "Song" },
      { ref_id: 105, group: "weblink" },
      { ref_id: 106, group: "maps" },
    ];
    const seq = mediaSequence(body, media);
    expect(seq.map((m) => m.ref_id)).toEqual([100, 101, 102, 103, 104]);
    expect(seq[1]?.title).toBe("Song");
  });

  it("finds the first paragraph and the section path of a paragraph", () => {
    expect(firstParagraph(body)).toMatchObject({ t: "p", id: 1 });
    expect(sectionPathOf(body, 7)).toEqual([2, 3]);
    expect(sectionPathOf(body, 2)).toEqual([1]);
    expect(sectionPathOf(body, 99)).toBeNull();
  });

  it("walks sections with their depth", () => {
    expect(walkSections(body).map((s) => [s.section.id, s.depth])).toEqual([
      [1, 1],
      [2, 1],
      [3, 2],
    ]);
  });
});

describe("content model", () => {
  it("prints the cross-reference prefixes of content-schema §2", () => {
    expect(linkPrefix("see-also")).toBe("See also ");
    expect(linkPrefix("see-embed")).toBe("see ");
    expect(linkPrefix("paren")).toBe("(");
    expect(linkSuffix("see-also-paren")).toBe(")");
    expect(linkPrefix("qv")).toBe("");
  });

  it("flattens inline content to text", () => {
    expect(plainText(["a ", { t: "frac", num: "1", den: "2" }, { t: "br" }, { t: "strong", c: ["b"] }])).toBe("a 1/2 b");
    expect(plainText([{ t: "list", style: "bullet", items: [["x"], ["y"]] }])).toBe("x y");
  });
});

describe("links", () => {
  it("maps items, paragraphs and Kids mode", () => {
    expect(itemHref(5)).toBe("/item/5");
    expect(itemHref(5, 12)).toBe("/item/5#p12");
    expect(itemHref(5, null, true)).toBe("/kids/item/5");
    expect(rewriteHref("/item/7#p1", true)).toBe("/kids/item/7#p1");
    expect(rewriteHref("/atlas/place/3", true)).toBe("/atlas/place/3");
    expect(kidsUrl("/dictionary/DICTA/1", true)).toBe("/kids/dictionary/DICTA/1");
    expect(kidsUrl("/item/1", false)).toBe("/item/1");
  });

  it("maps application URLs", () => {
    expect(appUrlHref("msencarta://view=MapView/cmd=Map/eid=45")).toBe(`/atlas?url=${encodeURIComponent("msencarta://view=MapView/cmd=Map/eid=45")}`);
    expect(appUrlHref("msencarta://view=ERSArticleView/id=123/display=article")).toBe("/item/123");
    expect(appUrlHref("http://example.com")).toBeNull();
  });

  it("reads the atlas starting view from the route", () => {
    expect(atlasInitialPlace("place/45", new URLSearchParams())).toEqual({ entity: 45 });
    expect(atlasInitialPlace("", new URLSearchParams("url=msencarta%3A%2F%2Fview%3DMapView"))).toBe("msencarta://view=MapView");
    expect(atlasInitialPlace("", new URLSearchParams("lat=1&lon=2"))).toEqual({ lat: 1, lon: 2, altitudeKm: undefined });
    expect(atlasInitialPlace("", new URLSearchParams())).toBeUndefined();
  });

  it("builds query strings without empty values", () => {
    expect(query({ q: "a b", kind: undefined, prefix: true, limit: 5, x: false })).toBe("?q=a+b&prefix=1&limit=5");
    expect(query({})).toBe("");
  });
});

describe("media selection", () => {
  const audio: Item = {
    ref_id: 1,
    kind: "audio",
    files: [
      { role: "thumb", file_name: "a.jtn", url: "/media/file/a.jtn" },
      {
        role: "audio",
        file_name: "a.wma",
        url: "/media/file/a.wma",
        media: {
          sha256: "x",
          kind: "audio",
          variants: { original: { url: "/o" }, opus: { url: "/m/x/opus", mime: "audio/ogg" }, aac: { url: "/m/x/aac" } },
        },
      },
      { role: "cc", file_name: "a.smi", url: "/media/file/a.smi", media: { sha256: "y", kind: "captions", variants: { vtt: { url: "/m/y/vtt" } } } },
    ],
  };

  it("prefers Opus, then AAC; never the WMA original", () => {
    expect(audioSources(fileByRole(audio, "audio")).map((s) => s.src)).toEqual(["/m/x/opus", "/m/x/aac"]);
    expect(captionTracks(audio).map((t) => t.src)).toEqual(["/m/y/vtt"]);
    expect(hasAudio(audio)).toBe(true);
  });

  it("treats MIDI as playable", () => {
    const midi: Item = { ref_id: 2, kind: "audio", files: [{ role: "audio", file_name: "t.mid", url: "/media/file/t.mid", media: { sha256: "z", kind: "midi", format: "midi", variants: { original: { url: "/m/z/original" } } } }] };
    expect(hasAudio(midi)).toBe(true);
  });

  it("classifies items for the viewer", () => {
    expect(mediaClass({ kind: "video" })).toBe("video");
    expect(mediaClass({ kind: "audio-set" })).toBe("audio");
    expect(mediaClass({ kind: "facts" })).toBe("table");
    expect(mediaClass({ kind: "media", media_type: "image" })).toBe("image");
    expect(mediaClass({ kind: "virtual-tour-node" })).toBe("tour");
    expect(mediaClass({ kind: "sidebar" })).toBe("text");
    expect(tourIdFor(42, [{ id: "00042", contentId: 42 }])).toBe("00042");
    expect(uniqueByRef([{ ref_id: 1 }, { ref_id: 2 }, { ref_id: 1 }]).length).toBe(2);
  });
});

describe("home screens", () => {
  const home: Item = {
    ref_id: 1,
    kind: "home-page",
    media: [
      { ref_id: 10, group: "hthm", sequence: 1, title: " " },
      { ref_id: 11, group: "home", sequence: 2, title: "History", kind: "home-page" },
      { ref_id: 12, group: "hthm", sequence: 3, title: "Top" },
      { ref_id: 13, group: "article", sequence: 4, title: "Zebra" },
      { ref_id: 14, group: "feat", sequence: 5, title: "Encarta Kids" },
      { ref_id: 15, group: "hthm", sequence: 99, title: "_logo_" },
      { ref_id: 16, group: "hsbg", sequence: 100, title: "Background" },
    ],
  };

  it("splits links at separators and skips theme icons", () => {
    const s = homeSections(home);
    expect(s.map((x) => [x.title, x.links.map((l) => l.ref_id)])).toEqual([
      [undefined, [11]],
      ["Top", [13, 14]],
    ]);
    expect(themeIcons(home).get("logo")?.ref_id).toBe(15);
  });

  it("routes home links", () => {
    expect(homeLinkHref({ ref_id: 11, group: "home" })).toBe("/browse/11");
    expect(homeLinkHref({ ref_id: 11, group: "home" }, true)).toBe("/kids/browse/11");
    expect(homeLinkHref({ ref_id: 14, group: "feat", title: "Encarta Kids" })).toBe("/kids");
    expect(homeLinkHref({ ref_id: 13, group: "article" })).toBe("/item/13");
  });
});

describe("quick facts", () => {
  it("uses a facts table's entries as they are", () => {
    const e = quickEntries({ entries: [{ name: ["Class"], value: ["Mammalia"] }] });
    expect(e).toHaveLength(1);
  });

  it("takes the quick entries of a Facts and Figures table with their continuation values", () => {
    const e = quickEntries({
      sections: [
        {
          entries: [
            { name: ["Capital"], value: ["X"], quick: true },
            { name: ["Area"], value: ["1 km"], quick: true, entries: [{ name: [], value: ["0.4 mi"] }] },
            { name: ["Other"], value: ["no"] },
          ],
        },
      ],
    });
    expect(e.map((x) => plainText(x.value))).toEqual(["X", "1 km", "0.4 mi"]);
  });
});

describe("timeline", () => {
  const ev = (seq: number, year: number, importance = 0): TimelineEvent => ({ seq, title: `E${seq}`, start: { year }, importance });

  it("formats dates", () => {
    expect(formatDate({ year: 1789, month: 7, day: 14 })).toBe("July 14, 1789");
    expect(formatDate({ year: -3000, circa: true })).toBe("c. 3,000 BC");
    expect(formatDate({ year: 79 })).toBe("AD 79");
    expect(formatDate({ year: -65_000_000 })).toBe("65 million years ago");
    expect(formatEventDate({ start: { year: 1756 }, end: { year: 1763 } })).toBe("1756 – 1763");
    expect(yearLabel(-2_000_000_000)).toBe("2 bya");
  });

  it("chooses round tick steps", () => {
    expect(tickStep(100, 10)).toBe(10);
    expect(tickStep(110, 8)).toBe(20);
    expect(ticks(1900, 2000, 5)).toEqual([1900, 1920, 1940, 1960, 1980, 2000]);
  });

  it("puts overlapping labels in different lanes, most important first", () => {
    const { placed, hidden } = placeEvents([ev(1, 1900), ev(2, 1901, 5), ev(3, 1950)], 1900, 2000, 1000, 170, 2);
    const lane = (s: number) => placed.find((p) => p.ev.seq === s)?.lane;
    expect(lane(2)).toBe(0);
    expect(lane(1)).toBe(1);
    expect(lane(3)).toBe(0);
    expect(hidden).toHaveLength(0);
    const tight = placeEvents([ev(1, 1900), ev(2, 1900), ev(3, 1900)], 1900, 2000, 1000, 170, 2);
    expect(tight.hidden).toHaveLength(1);
  });

  it("keeps labels inside the band", () => {
    const { placed } = placeEvents([ev(1, 1800), ev(2, 2000)], 1900, 2000, 1000, 170, 3);
    expect(placed.map((p) => p.x)).toEqual([0, 830]);
  });

  it("zooms and pans within the data range", () => {
    expect(zoom(1900, 2000, 0.5, -5000, 2010)).toEqual([1925, 1975]);
    expect(zoom(1900, 2000, 4, 1800, 2010)).toEqual([1800, 2010]);
    expect(pan(1900, 2000, 0.5, -5000, 2010)).toEqual([1910, 2010]);
    expect(categoryIndex(4)).toBe(2);
    expect(categoryIndex(0)).toBe(6);
  });
});

describe("dictionary", () => {
  it("reverses language pairs and knows the grammar codes", () => {
    expect(reversePair("ENG_FRA")).toBe("FRA_ENG");
    expect(reversePair("DICTA")).toBe("DICTA");
    expect(GRAMMAR_CODES.vi).toBe("intransitive verb");
  });
});

describe("WebVTT cues", () => {
  it("reads timings and strips tags", () => {
    const cues = parseVtt("WEBVTT\n\n00:00:01.000 --> 00:00:02.500\n<c.source>Narrator</c>\nHello\n\n00:01:00.000 --> 00:01:01.000 align:start\nBye\n");
    expect(cues).toEqual([
      { start: 1, end: 2.5, text: "Narrator\nHello" },
      { start: 60, end: 61, text: "Bye" },
    ]);
  });
});

/** Builds a Standard MIDI File from track byte arrays (synthetic). */
function smf(format: number, division: number, tracks: number[][]): Uint8Array {
  const out: number[] = [];
  const u32 = (n: number) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
  out.push(...[..."MThd"].map((c) => c.charCodeAt(0)), ...u32(6), 0, format, 0, tracks.length, division >> 8, division & 255);
  for (const t of tracks) out.push(...[..."MTrk"].map((c) => c.charCodeAt(0)), ...u32(t.length), ...t);
  return new Uint8Array(out);
}

describe("MIDI reader", () => {
  it("reads notes with running status, tempo changes and program changes", () => {
    // 480 ppq. Track 0: tempo 1,000,000 µs/quarter. Track 1: program 40 on ch 0, two notes.
    const t0 = [0x00, 0xff, 0x51, 0x03, 0x0f, 0x42, 0x40, 0x00, 0xff, 0x2f, 0x00];
    const t1 = [
      0x00, 0xc0, 40, // program change
      0x00, 0x90, 60, 100, // note on C4
      0x83, 0x60, 62, 90, // +480 ticks, running status: note on D4
      0x83, 0x60, 60, 0, // +480: C4 off (velocity 0)
      0x00, 0x80, 62, 0, // D4 off
      0x00, 0xff, 0x2f, 0x00,
    ];
    const song = parseMidi(smf(1, 480, [t0, t1]));
    expect(song.format).toBe(1);
    expect(song.notes).toHaveLength(2);
    const [c4, d4] = song.notes;
    expect(c4).toMatchObject({ note: 60, velocity: 100, program: 40, channel: 0 });
    expect(c4!.t).toBeCloseTo(0);
    expect(c4!.d).toBeCloseTo(2); // 960 ticks at 1 s per quarter
    expect(d4!.t).toBeCloseTo(1);
    expect(d4!.d).toBeCloseTo(1);
    expect(song.duration).toBeCloseTo(2);
  });

  it("rejects data that is not MIDI", () => {
    expect(() => parseMidi(new Uint8Array([1, 2, 3, 4]))).toThrow();
  });

  it("knows note frequencies", () => {
    expect(midiFrequency(69)).toBe(440);
    expect(midiFrequency(81)).toBeCloseTo(880);
  });
});
