import { describe, expect, it } from "vitest";
import type { LayerSpecification } from "maplibre-gl";
import { classNames, hitClassesAt, hitLayers, hitOrder, hitRanges, interactiveRanges, isHoverLayer, layerClasses, layerState, menuExcludedClasses, statLayer } from "../layers";
import { syntheticStyle } from "./fixtures";

const s = syntheticStyle();
const byId = (id: string) => s.layers.find((l) => l.id === id) as LayerSpecification;

describe("layer classes", () => {
  it("reads == and in filters with their names", () => {
    expect(layerClasses(byId("fill-areas-0"))).toEqual({ ids: [62, 64], names: ["Sovereign", "Admin 1"] });
    expect(layerClasses(byId("symbol-points-8")).ids).toEqual([0]);
    expect(layerClasses(byId("background")).ids).toEqual([]);
    expect(classNames(s).get(390)).toBe("CountryRegion - Yellow");
  });
  it("finds hover layers", () => {
    expect(isHoverLayer(byId("hover-areas-5"))).toBe(true);
    expect(isHoverLayer(byId("fill-areas-1"))).toBe(false);
  });
});

describe("class overrides (mad-styling.md §4.3)", () => {
  const none = { on: new Set<number>(), off: new Set<number>() };
  it("keeps the rule's visibility by default", () => {
    expect(layerState(byId("fill-areas-1"), none).visible).toBe(true);
    expect(layerState(byId("fill-areas-0"), none).visible).toBe(false);
  });
  it("turns an optional class on with a narrowed filter", () => {
    const st = layerState(byId("fill-areas-0"), { on: new Set([64]), off: new Set() });
    expect(st.visible).toBe(true);
    expect(st.filter).toEqual(["==", ["get", "class"], 64]);
  });
  it("keeps the draw-over mask test when narrowing the class test", () => {
    const l = byId("line-lines-11");
    expect(layerState(l, { on: new Set(), off: new Set() }).filter).toEqual(["all", ["==", ["get", "class"], 70], ["==", ["get", "water"], true]]);
    expect(layerClasses(l).ids).toEqual([70]);
  });
  it("turns a class off, hiding the layer when nothing is left", () => {
    const l = byId("line-lines-7");
    expect(layerState(l, { on: new Set(), off: new Set([80]) }).filter).toEqual(["==", ["get", "class"], 81]);
    expect(layerState(l, { on: new Set(), off: new Set([80, 81]) }).visible).toBe(false);
  });
});

describe("added layers", () => {
  it("hit-tests the classes the rules make interactive", () => {
    expect(interactiveRanges(s).get(64)).toEqual([[0.97, 5, 2, 0]]);
    expect(interactiveRanges(s).get(80)).toEqual([[0, 24, 1, 2]]);
    const none = { on: new Set<number>(), off: new Set<number>() };
    const r = hitRanges(s, none);
    // 62 by default; 64 only when turned on; 999 even though no layer draws it (V11).
    expect(r.get(62)).toEqual([
      [0.97, 4, 0],
      [4, 16.59, 1],
    ]);
    expect(r.has(64)).toBe(false);
    expect(r.has(999)).toBe(true);
    expect(hitClassesAt(r, 2).get(80)).toBe(2);
    expect(hitClassesAt(r, 2).get(62)).toBe(0);
    expect(hitClassesAt(r, 6).get(62)).toBe(1);
    expect(hitClassesAt(r, 0.5).has(62)).toBe(false);
    const on = hitRanges(s, { on: new Set([64]), off: new Set() });
    expect(on.get(64)).toEqual([[0.97, 5, 0]]);
    expect(hitRanges(s, { on: new Set(), off: new Set([62]) }).has(62)).toBe(false);
    const h = hitLayers(s, r);
    expect(h.map((l) => l.id)).toContain("encarta-hit-areas-earth-0");
    expect(h.every((l) => (l as { paint: Record<string, unknown> }).paint["fill-opacity"] === 0)).toBe(true);
    expect(JSON.stringify(h.find((l) => (l as { minzoom?: number }).minzoom === 0.97))).toContain('"literal",[62]');
    // Both parts of split polygons are hit-tested (rankHits applies the hit sides).
    expect(h.some((l) => JSON.stringify((l as { filter?: unknown }).filter).includes('"water"'))).toBe(false);
  });
  it("leaves out the classes of pen group 134 (time zones) from the click menu", () => {
    const none = { on: new Set<number>(), off: new Set<number>() };
    const withZones = { ...s, metadata: { ...(s.metadata ?? {}), "encarta:menu_excluded_classes": [80] } };
    expect(hitRanges(s, none).has(80)).toBe(true);
    expect(hitRanges(withZones, none).has(80)).toBe(false);
    expect(hitRanges(withZones, none).has(62)).toBe(true);
    expect(menuExcludedClasses(withZones).has(80)).toBe(true);
  });
  it("reads the click menu's sort data", () => {
    const m = { ...s, metadata: { ...(s.metadata ?? {}), "encarta:class_scale": { "62": 420, "64": 250 }, "encarta:routable_classes": [474] } };
    const o = hitOrder(m);
    expect([...o.scale]).toEqual([[62, 420], [64, 250]]);
    expect(o.routable.has(474)).toBe(true);
    expect(hitOrder(s).scale.size).toBe(0);
  });
  it("colours countries from feature-state", () => {
    const l = statLayer("earth") as { paint: Record<string, unknown>; "source-layer": string };
    expect(JSON.stringify(l.paint["fill-color"])).toContain('"feature-state","stat"');
    expect(l["source-layer"]).toBe("areas");
  });
});
