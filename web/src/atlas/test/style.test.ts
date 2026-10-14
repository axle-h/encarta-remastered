import { describe, expect, it } from "vitest";
import { defaultEndpoints } from "../data";
import { buildStyle, defaultVisibleClasses, styleMeta } from "../style";
import { syntheticStyle } from "./fixtures";

const ep = defaultEndpoints({ origin: "http://h" });
const none = { on: new Set<number>(), off: new Set<number>() };
const get = (s: ReturnType<typeof buildStyle>, id: string) => s.layers.find((l) => l.id === id) as unknown as { layout: Record<string, unknown>; paint: Record<string, unknown>; filter?: unknown };

describe("buildStyle", () => {
  it("resolves URLs, sets the projection and adds the client layers", () => {
    const s = buildStyle(syntheticStyle(), { ep, projection: "flat", overrides: none, labelScale: 1 });
    expect(s.projection).toEqual({ type: "mercator" });
    expect(s.glyphs).toBe("http://h/out/atlas/glyphs/{fontstack}/{range}.pbf");
    const ids = s.layers.map((l) => l.id);
    expect(ids.indexOf("encarta-stat-fill")).toBe(ids.indexOf("line-lines-6") - 1);
    expect(ids.at(-1)).toMatch(/^encarta-hit-areas-earth-detail-/);
    expect(get(s, "encarta-stat-fill").layout.visibility).toBe("none");
  });
  it("applies label size and no-labels", () => {
    const big = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 1.25 });
    expect(get(big, "symbol-points-8").layout["text-size"]).toBe(17.5);
    const no = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 0 });
    expect(get(no, "symbol-labels-10").layout.visibility).toBe("none");
    expect(get(no, "symbol-points-8").layout["text-field"]).toBeUndefined();
    expect(get(no, "symbol-points-8").layout["icon-image"]).toBe("bm10");
  });
  it("shows an optional symbol's label when its class is turned on", () => {
    const s = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: { on: new Set([7]), off: new Set() }, labelScale: 1 });
    expect(get(s, "symbol-points-9").layout.visibility).toBe("visible");
    expect(get(s, "symbol-points-9").paint["text-opacity"]).toBe(1);
  });
  it("swaps the raster for a client-side or baked LOOK", () => {
    const pal = { HYPSO: { looks: {}, max_level: 8, min_altitude_km: 150 } };
    const s = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 1, look: { raster: "HYPSO", look: "LBU_BORDERS", baked: false }, palettes: pal });
    expect(s.sources.raster).toMatchObject({ tiles: ["encarta-look://HYPSO/LBU_BORDERS/{z}/{x}/{y}"], maxzoom: 9 });
    const b = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 1, look: { raster: "HYPSO", look: "HYPSOFLAT", baked: true } });
    expect(s.sources.raster).not.toEqual(b.sources.raster);
    expect((b.sources.raster as { url: string }).url).toBe("pmtiles://http://h/out/atlas/raster/hypso/hypsoflat.pmtiles");
  });
  it("moves every zoom range by the view's shift", () => {
    const s = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 1, shift: -0.5 });
    expect((s.layers.find((l) => l.id === "hover-areas-5") as { minzoom: number; maxzoom: number }).minzoom).toBeCloseTo(0.08, 6);
    expect((s.layers.find((l) => l.id === "line-lines-6") as { minzoom: number }).minzoom).toBe(2.5);
  });
  it("re-tiles the archives into Miller view coordinates in the flat view", () => {
    const pal = { HYPSO: { looks: {}, max_level: 8, min_altitude_km: 150 } };
    const s = buildStyle(syntheticStyle(), { ep, projection: "flat", overrides: none, labelScale: 1, palettes: pal });
    const earth = s.sources.earth as { tiles: string[]; maxzoom: number };
    expect(earth.tiles[0]).toBe(`encarta-miller-mvt://8/${encodeURIComponent("http://h/out/atlas/vector/earth.pmtiles")}/{z}/{x}/{y}`);
    expect((s.sources["earth-detail"] as { minzoom: number }).minzoom).toBe(10);
    const r = s.sources.raster as { tiles: string[]; maxzoom: number };
    expect(r.tiles[0]).toMatch(/^encarta-miller-raster:\/\/HYPSO\/HYPSO\//);
    expect(r.maxzoom).toBe(9);
  });
  it("highlights one entity under the symbols", () => {
    const s = buildStyle(syntheticStyle(), { ep, projection: "globe", overrides: none, labelScale: 1, highlight: { entity: 25262, color: "#ff0000" } });
    const ids = s.layers.map((l) => l.id);
    const h = ids.indexOf("encarta-highlight-fill-earth");
    expect(h).toBeGreaterThan(ids.indexOf("line-lines-7"));
    expect(h).toBeLessThan(ids.indexOf("symbol-points-8"));
    expect((s.layers[h] as { filter: unknown }).filter).toEqual(["==", ["id"], 25262]);
  });
  it("reads the style metadata", () => {
    const m = styleMeta(syntheticStyle());
    expect(m).toMatchObject({ styleId: 12, raster: "HYPSO", minzoom: 0.59, maxzoom: 8.97, pOverM: 3135, pOverMPerHeightPx: 6.05 });
    expect(m.minRects).toHaveLength(1);
    expect(defaultVisibleClasses(syntheticStyle()).has(7)).toBe(false);
    expect(defaultVisibleClasses(syntheticStyle()).has(0)).toBe(true);
  });
});
