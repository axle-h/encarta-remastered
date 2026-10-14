import { describe, expect, it } from "vitest";
import { defaultEndpoints, dataFile, linkLabel, sortLinks, primaryArticle, rasterArchive, resolveDataUrl, resolveStyleUrls, makeTransformRequest } from "../data";
import { syntheticStyle } from "./fixtures";

const ep = defaultEndpoints({ origin: "http://localhost:5180" });

describe("URL resolution", () => {
  it("maps the converter's /atlas root to dataUrl and makes URLs absolute", () => {
    expect(resolveDataUrl("/atlas/sprites/sprite", ep)).toBe("http://localhost:5180/out/atlas/sprites/sprite");
    expect(resolveDataUrl("/atlas/glyphs/{fontstack}/{range}.pbf", ep)).toBe("http://localhost:5180/out/atlas/glyphs/{fontstack}/{range}.pbf");
    expect(resolveDataUrl("pmtiles:///atlas/vector/earth.pmtiles", ep)).toBe("pmtiles://http://localhost:5180/out/atlas/vector/earth.pmtiles");
    expect(resolveDataUrl("/media/file/x.jpg", ep)).toBe("http://localhost:5180/media/file/x.jpg");
  });
  it("keeps URLs the server already made absolute, except same-origin /atlas/ (a client route behind Vite)", () => {
    expect(resolveDataUrl("pmtiles://https://enc.example/atlas/vector/earth.pmtiles", ep)).toBe("pmtiles://https://enc.example/atlas/vector/earth.pmtiles");
    expect(resolveDataUrl("https://enc.example/atlas/sprites/sprite", ep)).toBe("https://enc.example/atlas/sprites/sprite");
    expect(resolveDataUrl("http://localhost:5180/atlas/sprites/sprite", ep)).toBe("http://localhost:5180/out/atlas/sprites/sprite");
    expect(resolveDataUrl("pmtiles://http://localhost:5180/atlas/vector/moon.pmtiles", ep)).toBe("pmtiles://http://localhost:5180/out/atlas/vector/moon.pmtiles");
  });
  it("honours a custom dataUrl", () => {
    const e2 = defaultEndpoints({ origin: "https://x.org", dataUrl: "/atlas/" });
    expect(resolveDataUrl("/atlas/sprites/sprite", e2)).toBe("https://x.org/atlas/sprites/sprite");
    expect(dataFile("raster/palettes.json", e2)).toBe("https://x.org/atlas/raster/palettes.json");
    expect(rasterArchive("HYPSO", "LBU_BORDERS", e2)).toBe("https://x.org/atlas/raster/hypso/lbu_borders.pmtiles");
  });
  it("rewrites a whole style without touching the original", () => {
    const raw = syntheticStyle();
    const s = resolveStyleUrls(raw, ep);
    expect(s.sprite).toBe("http://localhost:5180/out/atlas/sprites/sprite");
    expect((s.sources.earth as { url: string }).url).toBe("pmtiles://http://localhost:5180/out/atlas/vector/earth.pmtiles");
    expect((raw.sources.earth as { url: string }).url).toBe("pmtiles:///atlas/vector/earth.pmtiles");
    expect(makeTransformRequest(ep)("/atlas/sprites/sprite.json").url).toBe("http://localhost:5180/out/atlas/sprites/sprite.json");
    expect(makeTransformRequest(ep)("https://a.b/c").url).toBe("https://a.b/c");
  });
});

describe("place links", () => {
  const links = [
    { kind: 1, kind_label: "flag-and-anthem", ref_id: 1, title: "Flag of Testland" },
    { kind: 8, kind_label: "article", ref_id: 2, title: "Testland" },
    { kind: 69, kind_label: "photo:landscape", ref_id: 3, title: "Hills" },
  ];
  it("picks the article for Read Article and labels media", () => {
    expect(primaryArticle(links)?.ref_id).toBe(2);
    expect(primaryArticle([])).toBeUndefined();
    expect(linkLabel(links[1]!)).toBe("Testland");
    expect(linkLabel(links[2]!)).toBe("Hills — Photo (landscape)");
    expect(linkLabel(links[0]!)).toBe("Flag of Testland");
    expect(linkLabel({ kind: 5, kind_label: "photo:general", ref_id: 5, title: "Photo of hills" })).toBe("Photo of hills");
    expect(sortLinks(links).map((l) => l.ref_id)).toEqual([2, 1, 3]);
    expect(linkLabel({ kind: 99, kind_label: "unknown-99", ref_id: 4 })).toBe("Media");
  });
});
