import { describe, expect, it } from "vitest";
import { applyLut, bakedLooks, buildLut, lookSource, lookTileUrl, parseLookUrl } from "../recolour";

describe("palette recolouring", () => {
  it("packs #rrggbb as little-endian RGBA and maps values", () => {
    const lut = buildLut(["#000000", "#ff8000", "#0000ff80"], new Set([0]));
    expect(lut[0]).toBe(0); // transparent
    const out = applyLut(new Uint8Array([1, 2, 0, 3]), lut);
    const bytes = new Uint8Array(out.buffer);
    expect([...bytes.subarray(0, 4)]).toEqual([0xff, 0x80, 0x00, 0xff]);
    expect([...bytes.subarray(4, 8)]).toEqual([0x00, 0x00, 0xff, 0x80]);
    expect([...bytes.subarray(8, 16)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0]); // index 0 transparent, index 3 undefined
  });
  it("builds and parses look tile URLs", () => {
    const t = lookTileUrl("hypso", "lbu_borders");
    expect(t).toBe("encarta-look://HYPSO/LBU_BORDERS/{z}/{x}/{y}");
    expect(parseLookUrl("encarta-look://HYPSO/LBU_BORDERS/3/4/5")).toEqual({ raster: "HYPSO", look: "LBU_BORDERS", z: 3, x: 4, y: 5 });
    expect(parseLookUrl("pmtiles://x")).toBeNull();
  });
  it("sizes the source from the pyramid depth", () => {
    const s = lookSource("HYPSO", "LBU_BORDERS", { HYPSO: { looks: {}, max_level: 8, min_altitude_km: 150 } });
    expect(s.maxzoom).toBe(9);
    expect(s.tileSize).toBe(256);
  });
  it("knows which looks are baked", () => {
    const b = bakedLooks([{ raster: "HYPSO", look: "POLITICAL" }, { raster: "MOON", look: "MOON" }]);
    expect(b.has("HYPSO/POLITICAL")).toBe(true);
    expect(b.has("HYPSO/LBU_BORDERS")).toBe(false);
  });
});
