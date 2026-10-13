import { describe, expect, it } from "vitest";
import { MENU_STYLE, findStyle, orderedStyles, styleBody, styleLabel, styleSlug, type StyleIndexEntry } from "../catalog";

const index: StyleIndexEntry[] = [
  { id: 0, name: "Atlas Default", file: "styles/atlas-default.json", raster: "HYPSO", look: "HYPSO", default: true },
  { id: 10, name: "Atlas Political", file: "styles/atlas-political.json", raster: "HYPSO", look: "POLITICAL" },
  { id: 16, name: "Atlas Moon", file: "styles/atlas-moon.json", raster: "MOON", look: "MOON" },
  { id: 31, name: "VE Road", file: "styles/ve-road.json", raster: "HYPSO", look: "GENERALFLAT" },
];

describe("style catalog", () => {
  it("finds styles by id, slug, name and caption", () => {
    expect(findStyle(index, 10)?.id).toBe(10);
    expect(findStyle(index, "10")?.id).toBe(10);
    expect(findStyle(index, "atlas-moon")?.id).toBe(16);
    expect(findStyle(index, "VE Road")?.id).toBe(31);
    expect(findStyle(index, "political")?.id).toBe(10);
    expect(findStyle(index, "nope")?.id).toBe(0);
    expect(findStyle(index, undefined)?.id).toBe(0);
  });
  it("orders the carousel like Encarta and keeps the rest", () => {
    const { menu, more } = orderedStyles(index);
    expect(menu.map((e) => e.id)).toEqual([0, 10, 16]);
    expect(more.map((e) => e.id)).toEqual([31]);
    expect(styleLabel(index[0]!)).toBe("Comprehensive");
    expect(styleLabel(index[3]!)).toBe("VE Road");
    expect(styleSlug(index[1]!)).toBe("atlas-political");
    expect(styleBody(index[2]!)).toBe("moon");
  });
  it("maps msencarta style numbers to MapStyle ids", () => {
    expect(MENU_STYLE[1]).toBe(10);
    expect(MENU_STYLE[20]).toBe(16);
  });
});
