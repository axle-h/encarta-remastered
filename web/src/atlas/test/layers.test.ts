import { describe, expect, it } from "vitest";
import type { LayerSpecification } from "maplibre-gl";
import { classNames, hitLayers, isHoverLayer, layerClasses, layerState, statLayer } from "../layers";
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
  it("turns a class off, hiding the layer when nothing is left", () => {
    const l = byId("line-lines-7");
    expect(layerState(l, { on: new Set(), off: new Set([80]) }).filter).toEqual(["==", ["get", "class"], 81]);
    expect(layerState(l, { on: new Set(), off: new Set([80, 81]) }).visible).toBe(false);
  });
});

describe("added layers", () => {
  it("adds named-area hit layers per vector source, theme areas only where drawn", () => {
    expect(JSON.stringify(hitLayers(s, [7])[0])).toContain('"literal",[7]');
    const h = hitLayers(s);
    expect(h.map((l) => l.id)).toEqual(["encarta-hit-areas-earth", "encarta-hit-areas-earth-detail"]);
    expect((h[0] as { paint: Record<string, unknown> }).paint["fill-opacity"]).toBe(0);
  });
  it("colours countries from feature-state", () => {
    const l = statLayer("earth") as { paint: Record<string, unknown>; "source-layer": string };
    expect(JSON.stringify(l.paint["fill-color"])).toContain('"feature-state","stat"');
    expect(l["source-layer"]).toBe("areas");
  });
});
