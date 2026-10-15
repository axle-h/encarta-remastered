/** Unit tests for the parity-audit additions (docs/parity.md), on synthetic data. */
import { describe, expect, it } from "vitest";
import type { Item, TimelineEvent } from "./api/types";
import { SOURCE_LINE, withSourceLine } from "./app/commands";
import { STRIP, tipPlacement } from "./components/ArrowTip";
import { defaultFavoriteName } from "./app/AddFavoriteDialog";
import { addFavorite, addFolder, allFolders, moveFavorite, moveToFolder, parseFolders, removeFolder, renameFolder, pageTitle, parseFavorites, removeFavorite, renameFavorite } from "./app/favorites";
import { mediaClass } from "./media/pick";
import { archiveUrl, lifeDates, metaText, webLinkUrl } from "./pages/ItemKinds";
import { featureHref, itemRedirect, mapTrekUrl, topicTerm, translatePair } from "./pages/itemRoute";
import { eventWindow, topicWindow } from "./pages/timelineLayout";

describe("favorites", () => {
  it("adds newest first, renames instead of duplicating, removes and moves", () => {
    let l = addFavorite([], "/item/1", "One", 10);
    l = addFavorite(l, "/item/2", "Two", 20);
    expect(l.map((f) => f.path)).toEqual(["/item/2", "/item/1"]);
    l = addFavorite(l, "/item/1", "One again", 30);
    expect(l).toHaveLength(2);
    expect(l[1]).toEqual({ path: "/item/1", title: "One again", added: 10 });
    expect(moveFavorite(l, "/item/1", -1).map((f) => f.path)).toEqual(["/item/1", "/item/2"]);
    expect(moveFavorite(l, "/item/2", -1)).toBe(l);
    expect(removeFavorite(l, "/item/2").map((f) => f.path)).toEqual(["/item/1"]);
    expect(renameFavorite(l, "/item/2", "  ")).toBe(l);
    expect(renameFavorite(l, "/item/2", "Deux")[0]!.title).toBe("Deux");
    expect(addFavorite([], "/x", "   ", 1)[0]!.title).toBe("/x");
  });

  it("keeps folders: file, rename, delete with contents, reorder within a folder", () => {
    let l = addFavorite([], "/a", "A", 1, "Trips");
    l = addFavorite(l, "/b", "B", 2);
    l = addFavorite(l, "/c", "C", 3, "Trips");
    expect(l.map((f) => f.path)).toEqual(["/c", "/b", "/a"]);
    expect(moveFavorite(l, "/c", 1).map((f) => f.path)).toEqual(["/a", "/b", "/c"]);
    expect(moveFavorite(l, "/a", 1)).toBe(l);
    expect(moveToFolder(l, "/a")[2]).toEqual({ path: "/a", title: "A", added: 1 });
    expect(addFavorite(l, "/b", "B", 9, "Trips")[1]!.folder).toBe("Trips");
    expect(allFolders(["Empty"], l)).toEqual(["Empty", "Trips"]);
    expect(addFolder(["Trips"], " trips ")).toBeNull();
    expect(addFolder(["Trips"], "Work")).toEqual(["Trips", "Work"]);
    const r = renameFolder(["Trips"], l, "Trips", "Travel")!;
    expect(r.folders).toEqual(["Travel"]);
    expect(r.list.filter((f) => f.folder === "Travel")).toHaveLength(2);
    expect(renameFolder(["Trips", "Work"], l, "Trips", "work")).toBeNull();
    const d = removeFolder(["Trips"], l, "Trips");
    expect(d.folders).toEqual([]);
    expect(d.list.map((f) => f.path)).toEqual(["/b"]);
    expect(parseFolders('["A"," ","A",3,"B"]')).toEqual(["A", "B"]);
    expect(parseFavorites('[{"path":"/a","folder":"F"}]')[0]!.folder).toBe("F");
  });

  it("parses stored data defensively", () => {
    expect(parseFavorites(null)).toEqual([]);
    expect(parseFavorites("not json")).toEqual([]);
    expect(parseFavorites('{"path":"/a"}')).toEqual([]);
    expect(parseFavorites('[{"path":"/a","title":"A","added":5},{"path":"http://evil"},{"title":"x"},null,{"path":"/b"}]')).toEqual([
      { path: "/a", title: "A", added: 5 },
      { path: "/b", title: "/b", added: 0 },
    ]);
  });

  it("takes the page title without the app name", () => {
    expect(pageTitle("Zebra - Encarta")).toBe("Zebra");
    expect(pageTitle("Encarta")).toBe("Encarta");
  });
});

describe("copy with the source line", () => {
  it("puts Encarta's line under a selection, after a blank line under a whole article", () => {
    expect(withSourceLine("  ")).toBeNull();
    expect(withSourceLine("Striped.")).toBe(`Striped.\n${SOURCE_LINE}`);
    expect(withSourceLine("Zebra\n\nText.", true)).toBe(`Zebra\n\nText.\n\n${SOURCE_LINE}`);
    expect(SOURCE_LINE).toBe("Microsoft® Encarta® 2009. © 1993-2008 Microsoft Corporation. All rights reserved.");
  });
});

describe("arrow tooltips", () => {
  it("centre under the button, or move to a corner near the window edges (VM)", () => {
    expect(tipPlacement(88, 128, 41, 1024)).toEqual({ placement: "below", x: 87.5 });
    expect(tipPlacement(8, 46, 89, 1024)).toEqual({ placement: "below-right", x: 46 });
    expect(tipPlacement(988, 1014, 122, 1024)).toEqual({ placement: "below-left", x: 988 - 122 - STRIP });
  });
});

describe("Add to Favorites", () => {
  it("suggests Article - <title> for articles and the plain title otherwise", () => {
    expect(defaultFavoriteName("Zebra", true)).toBe("Article - Zebra");
    expect(defaultFavoriteName("Leaning Tower of Pisa", false)).toBe("Leaning Tower of Pisa");
  });
});

describe("item routes", () => {
  const item = (o: Partial<Item>): Item => ({ ref_id: 1, kind: "media", ...o });

  it("maps every feature record to its page", () => {
    expect(translatePair("BA2E")).toBe("ENG_SPA");
    expect(translatePair("BE2A")).toBe("SPA_ENG");
    expect(translatePair("BA2F")).toBe("ENG_FRA");
    expect(translatePair("BD2A")).toBe("GER_ENG");
    expect(translatePair("BI2A")).toBe("ITA_ENG");
    expect(translatePair("BA2A")).toBeNull();
    expect(translatePair("BX2X")).toBeNull();
    const cases: [string, string | null][] = [
      ["DICT", "/dictionary"],
      ["THES", "/thesaurus"],
      ["BX2X", "/translate"],
      ["BA2I", "/translate?pair=ENG_ITA"],
      ["Time", "/timeline"],
      ["GeoQ", "/games/geoquiz"],
      ["Kids", "/kids"],
      ["QART", "/search?kind=article"],
      ["QMAP", "/search?kind=map"],
      ["QPHO", "/media"],
      ["QQUO", "/search?kind=quotation"],
      ["QDCT", "/dictionary"],
      ["QTHE", "/thesaurus"],
      ["Prep", null],
      ["EONL", null],
    ];
    for (const [t, href] of cases) expect(featureHref(t), t).toBe(href);
    expect(itemRedirect(item({ kind: "feature", subtype: "THES" }))).toBe("/thesaurus");
    expect(itemRedirect(item({ kind: "feature", subtype: "EONL" }))).toBeNull();
  });

  it("sends topic timelines to the timeline's term view", () => {
    expect(topicTerm("Aztec Empire Topic Timeline")).toBe("Aztec Empire");
    expect(topicTerm("Timeline: 10th Century BC")).toBe("10th Century BC");
    expect(topicTerm("Dodo Becomes Extinct")).toBe("Dodo Becomes Extinct");
    expect(itemRedirect(item({ kind: "timeline-topic", title: "Women's Rights Topic Timeline" }))).toBe("/timeline?term=Women's%20Rights");
    expect(itemRedirect(item({ kind: "timeline-topic", title: "" }))).toBe("/timeline");
  });

  it("opens timeline events in the Timeline at the event", () => {
    const e: TimelineEvent = { seq: 1, title: "Jesus Christ", start: { year: -4 } };
    expect(itemRedirect(item({ ref_id: 77, kind: "timeline-event", timeline: e }))).toBe("/timeline?event=77");
    expect(itemRedirect(item({ ref_id: 77, kind: "timeline-event" }))).toBeNull();
    expect(eventWindow(e, -5000, 2010)).toEqual([-54, 46]);
    expect(eventWindow({ ...e, start: { year: 1939 }, end: { year: 1945 } }, -5000, 2010)).toEqual([1934, 1950]);
  });

  it("opens icon-only panoramas in their viewer and Map Trek stops in their trek", () => {
    const pano = item({ kind: "panorama", files: [{ role: "ticon", file_name: "pan.gif", url: "/m" }], appears_in: [{ ref_id: 9, group: "flash", kind: "interactive" }] });
    expect(itemRedirect(pano)).toBe("/item/9");
    expect(itemRedirect({ ...pano, files: [{ role: "image", file_name: "p.jpg", url: "/m" }] })).toBeNull();
    expect(itemRedirect({ ...pano, appears_in: [] })).toBeNull();
    expect(itemRedirect(item({ ref_id: 5, kind: "maptrek-stop", appears_in: [{ ref_id: 7, group: "maptrektour", kind: "maptrek-tour" }] }))).toBe("/item/7?stop=5");
    expect(itemRedirect(item({ kind: "article" }))).toBeNull();
  });

  it("builds the atlas URL of a Map Trek stop", () => {
    expect(mapTrekUrl({ lat: "-89.7", long: "0", alt: "14000", style: "16" })).toBe("msencarta://view=MapView/cmd=map/lat=-89.7/long=0/alt=14000/style=16");
    expect(mapTrekUrl({ lat: "10", long: "20" })).toBe("msencarta://view=MapView/cmd=map/lat=10/long=20");
    expect(mapTrekUrl({ lat: "x", long: "1" })).toBeNull();
    expect(mapTrekUrl(undefined)).toBeNull();
  });

  it("gives interactives their player whatever their icon", () => {
    expect(mediaClass({ kind: "interactive", media_type: "narr" })).toBe("interactive");
    expect(mediaClass({ kind: "interactive", media_type: "map" })).toBe("interactive");
    expect(mediaClass({ kind: "audio", media_type: "narr" })).toBe("audio");
    expect(mediaClass({ kind: "map", media_type: "map" })).toBe("map");
  });
});

describe("item kind helpers", () => {
  it("reads DATA meta values", () => {
    expect(metaText(["Professor of ", { t: "em", c: ["Music"] }])).toBe("Professor of Music");
    expect(metaText(undefined)).toBe("");
    expect(lifeDates({ born: ["1935"], flourished: ["0"] })).toBe("1935–");
    expect(lifeDates({ born: ["1657"], died: ["1757"] })).toBe("1657–1757");
    expect(lifeDates({ flourished: ["1200"] })).toBe("fl. 1200");
    expect(lifeDates({})).toBe("");
  });

  it("finds a web link's address and its archived copy", () => {
    expect(webLinkUrl({ meta: { url: ["http://example.com/a"] } })).toBe("http://example.com/a");
    expect(webLinkUrl({ meta: { url: ["javascript:alert(1)"] } })).toBeNull();
    expect(webLinkUrl({ meta: {} })).toBeNull();
    expect(archiveUrl("http://example.com/a")).toBe("https://web.archive.org/web/2008/http://example.com/a");
  });
});

describe("topic timeline window", () => {
  const ev = (start: number, end?: number): TimelineEvent => ({ seq: start, title: "e", start: { year: start }, end: end === undefined ? null : { year: end } });
  it("fits every event with a margin", () => {
    expect(topicWindow([ev(1300, 1350), ev(1500)], -5000, 2010)).toEqual([1280, 1520]);
    expect(topicWindow([ev(2000)], -5000, 2010)).toEqual([1995, 2005]);
    expect(topicWindow([], -5000, 2010)).toEqual([1900, 2010]);
    expect(topicWindow([ev(-4999)], -5000, 2010)).toEqual([-5000, -4994]);
  });
});

describe("panoramic tours", () => {
  it("reads scenes, hotspot rectangles, label points and destinations", async () => {
    const { childIds, destOf, isPanoramicTour, labelPoint, shedRect } = await import("./pages/panorama");
    expect(isPanoramicTour({ meta: { source_attrs: { type: "2DVT" } } })).toBe(true);
    expect(isPanoramicTour({ meta: { source_attrs: {} } })).toBe(false);
    expect(isPanoramicTour({ meta: { source_attrs: { group: "virttour", ticon: "vtour", type: "" } } })).toBe(true);
    expect(isPanoramicTour({ meta: { source_attrs: { group: "virttour", ticon: "vtour", type: "3DVT" } } })).toBe(false);
    expect(childIds({ children: [{ kind: "media", ref_id: 2 }, { kind: "article", ref_id: 3 }, { kind: "media", ref_id: 4 }] }, "media")).toEqual([2, 4]);
    expect(shedRect({ shed: [[727, 110], [692, 91]] })).toEqual({ x: 692, y: 91, w: 35, h: 19 });
    expect(shedRect({ shed: [[1, 1]] })).toBeNull();
    expect(shedRect({ shed: [[1, "x"], [2, 2]] })).toBeNull();
    expect(labelPoint({ source_attrs: { x: "1042", y: "102" } })).toEqual({ x: 1042, y: 102 });
    expect(labelPoint({ source_attrs: { x: "1042" } })).toBeNull();
    expect(destOf({ meta: { source_attrs: { destmedia: "461545494" } } })).toBe(461545494);
    expect(destOf({ meta: {} })).toBeNull();
  });
});
