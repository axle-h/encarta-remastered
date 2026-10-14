import { describe, expect, it } from "vitest";
import {
  MILLER_Y_MAX,
  VIEW_LAT_MAX,
  fromViewLat,
  mercWorldLat,
  mercWorldY,
  millerImageRect,
  millerLevel,
  millerY,
  realRows,
  realZoom,
  retile,
  toViewLat,
  zoomToFitHeight,
} from "../miller";
import { LINESTRING, POINT, POLYGON, area2, clipLine, clipRing, decodeTile, encodeTile, type Layer } from "../mvt";

describe("Miller view latitudes", () => {
  it("follow mad-styling.md §12.3 / VM V12 (y = 1.25 ln tan(45° + 0.4 φ))", () => {
    // V12: parallels at 112.76 px per Miller unit: 15° N 30 px above the equator, 60° N 135 px.
    expect(millerY(15) * 112.76).toBeCloseTo(30, 0);
    expect(millerY(60) * 112.76).toBeCloseTo(135, 0);
    expect(MILLER_Y_MAX).toBeCloseTo(2.3034, 3);
    expect(VIEW_LAT_MAX).toBeCloseTo(78.6, 1);
  });
  it("round-trip and keep the equator", () => {
    for (const lat of [-89, -60, -10, 0, 33.3, 85, 89.9]) expect(fromViewLat(toViewLat(lat))).toBeCloseTo(lat, 6);
    expect(toViewLat(0)).toBeCloseTo(0, 12);
    expect(mercWorldLat(mercWorldY(48.86))).toBeCloseTo(48.86, 9);
  });
  it("fits the whole map to the pane height (V12: 518-px pane)", () => {
    // At this zoom the Miller map is 518 px tall and 512·2^z wide: 1.97 px per degree.
    const z = zoomToFitHeight(518);
    expect((512 * 2 ** z) / 360).toBeCloseTo(1.968, 1);
  });
  it("finds the real tile rows of a view tile", () => {
    expect(realRows(1, 0, 1)).toEqual([0, 0]);
    // Near the pole a view row covers many Mercator rows; a coarser real zoom is read.
    const [a, b] = realRows(6, 11, 6);
    expect(b - a).toBeGreaterThanOrEqual(3);
    const zr = realZoom(6, 11, 8, 2);
    const [c, d] = realRows(6, 11, zr);
    expect(zr).toBeLessThan(6);
    expect(d - c + 1).toBeLessThanOrEqual(2);
    // Rows beyond the Miller poles clamp to the Mercator limit.
    expect(realRows(6, 4, 6)).toEqual([0, 0]);
  });
  it("crops Encarta's flat image affinely", () => {
    // Level 1 is 896 × 640: the view's top tile row starts (H − W)/2 = −128 px above the image.
    expect(millerImageRect(896, 640, 1, 1, 0)).toEqual({ sx: 448, sy: -128, size: 448 });
    const images = [0, 1, 2, 3].map((level) => ({ raster: "HYPSO", level, width: [512, 896, 1792, 3328][level]!, height: 0 }));
    expect(millerLevel(images, 1)?.level).toBe(0);
    expect(millerLevel(images, 3)?.level).toBe(3);
    expect(millerLevel(images, 6)?.level).toBe(3);
  });
});

describe("MVT codec", () => {
  const layer: Layer = {
    name: "areas",
    extent: 4096,
    features: [
      { id: 25262, type: POLYGON, props: { name: "Île", class: 7, water: true, w: 1.5, neg: -3 }, parts: [[[0, 0], [100, 0], [100, 100], [0, 100]].map((p) => [p[0]!, p[1]!] as [number, number]).reverse()] },
      { id: 161775207, type: LINESTRING, props: { name: "Loire" }, parts: [[[1, 2], [30, 40], [50, 45]]] },
      { type: POINT, props: {}, parts: [[[5, 6], [7, 8]]] },
    ],
  };
  it("round-trips ids, properties and geometry", () => {
    const back = decodeTile(encodeTile([layer]));
    expect(back).toHaveLength(1);
    expect(back[0]!.features).toEqual(layer.features);
  });
  it("clips rings and lines", () => {
    expect(Math.abs(area2(clipRing([[-10, -10], [10, -10], [10, 10], [-10, 10]], 0, 0, 100, 100)))).toBeCloseTo(200, 6);
    expect(clipLine([[-10, 5], [10, 5], [10, 200]], 0, 0, 100, 100)).toEqual([[[0, 5], [10, 5], [10, 100]]]);
  });
});

describe("re-tiling into view coordinates", () => {
  it("moves a point to its view latitude and drops what falls outside", () => {
    const lat = 48.86;
    const z = 4;
    const wy = mercWorldY(lat);
    const ry = Math.floor(wy * 16);
    const py = Math.round((wy * 16 - ry) * 4096);
    const src = { zr: z, x: 8, y: ry, layers: [{ name: "points", extent: 4096, features: [{ id: 1, type: POINT, props: { name: "Paris" }, parts: [[[100, py] as [number, number]]] }] }] };
    const vy = mercWorldY(toViewLat(lat)) * 16;
    const out = retile([src], z, 8, Math.floor(vy));
    const f = out[0]!.features[0]!;
    expect(f.props.name).toBe("Paris");
    expect(f.parts[0]![0]![1]).toBeCloseTo((vy - Math.floor(vy)) * 4096, -1);
    expect(retile([src], z, 8, Math.floor(vy) + 1)[0]!.features).toHaveLength(0);
  });
  it("keeps exterior rings positive and clips them to the view tile", () => {
    const ring: [number, number][] = [[0, 0], [4096, 0], [4096, 4096], [0, 4096]];
    const src = { zr: 2, x: 2, y: 1, layers: [{ name: "areas", extent: 4096, features: [{ id: 9, type: POLYGON, props: {}, parts: [ring] }] }] };
    const out = retile([src], 2, 2, 1);
    const r = out[0]!.features[0]!.parts[0]!;
    expect(area2(r)).toBeGreaterThan(0);
    expect(r.every(([x, y]) => x >= -64 && x <= 4160 && y >= -64 && y <= 4160)).toBe(true);
  });
});
