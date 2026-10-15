import { describe, expect, it } from "vitest";
import { hoverClassesAt, hoverTarget, rankHits, viewTitle, type QueriedFeature } from "../hit";
import { syntheticStyle } from "./fixtures";

const f = (id: number, sourceLayer: string, name: string, className: string, cls = 0, extra: Record<string, unknown> = {}): QueriedFeature => ({
  id,
  sourceLayer,
  layer: { id: `l-${sourceLayer}`, type: sourceLayer === "points" ? "symbol" : "fill" },
  properties: { name, class_name: className, class: cls, ...extra },
});

describe("hit ranking (V11 menu order)", () => {
  const hits = rankHits([
    f(1, "areas", "France", "Sovereign", 62),
    f(2, "areas", "Western Europe", "Region"),
    f(3, "lines", "Loire", "River"),
    f(4, "points", "Limoges", "PPL - town"),
    f(1, "labels", "France", "Sovereign", 62),
    f(5, "areas", "", "Hypsometric (0 to 50 m)"),
    f(6, "areas", "Nowhere", "Region", 0, { anonymous: true }),
    f(7, "areas", "Auvergne", "Admin 1", 64),
    f(8, "areas", "World", "World"),
    f(10, "lines", "15°", "Graticule - 15 deg - meridian"),
    f(11, "lines", "Tropic of Cancer", "Graticule - tropics, circles"),
  ]);
  it("lists points, lines, areas, countries last, once per entity", () => {
    expect(hits.map((h) => h.name)).toEqual(["Limoges", "Loire", "15°", "Tropic of Cancer", "Western Europe", "Auvergne", "France", "World"]);
  });
  it("applies the hit sides: water classes only over water, land classes not with them (mad-styling.md §4.4)", () => {
    const sides = new Map([
      [3, 1], // CountryRegion: land
      [5, 1], // Admin 1: land
      [90, 2], // water body
      [54, 0], // park: anywhere
    ]);
    // Off the Breton coast: the Bay is hit, so France and Brittany (their water parts) are left out.
    const sea = rankHits(
      [
        f(84, "areas", "France", "CountryRegion - Purple", 3, { water: true }),
        f(4840, "areas", "Brittany", "Admin1 - Purple - 2 Med Dark", 5, { water: true }),
        f(4019, "areas", "Bay of Biscay", "Water label - major", 90, { water: true }),
        f(77, "areas", "Marine Park", "Park - national", 54, { water: true }),
      ],
      undefined,
      sides,
    );
    expect(sea.map((h) => h.name)).toEqual(["Bay of Biscay", "Marine Park"]);
    // On Belle-Île: the Bay's land part is not hit, so the land classes stay.
    const island = rankHits([f(84, "areas", "France", "CountryRegion - Purple", 3), f(4019, "areas", "Bay of Biscay", "Water label - major", 90)], undefined, sides);
    expect(island.map((h) => h.name)).toEqual(["France"]);
    // Off Jersey, where no water class is hit: the land class is listed over the sea.
    const jersey = rankHits([f(123, "areas", "Jersey (U.K.)", "Dependent - Red - Small", 3, { water: true })], undefined, sides);
    expect(jersey.map((h) => h.name)).toEqual(["Jersey (U.K.)"]);
  });
  it("keeps only the classes in the style's hit list", () => {
    const only = rankHits([f(1, "areas", "France", "Sovereign", 62), f(10, "lines", "15°", "Graticule - 15 deg - meridian", 77)], undefined, new Set([62]));
    expect(only.map((h) => h.name)).toEqual(["France"]);
  });
  it("picks the first hit with a highlight pen", () => {
    const hoverable = hoverClassesAt(syntheticStyle(), 3);
    expect([...hoverable].sort()).toEqual([390, 62]);
    expect(hoverTarget(hits, hoverable)?.name).toBe("France");
    expect(hoverClassesAt(syntheticStyle(), 6).size).toBe(0);
  });
  it("titles the view after the country and continent under the centre", () => {
    const h = rankHits([f(1, "areas", "Germany", "Sovereign", 62), f(9, "areas", "Europe", "Continent")]);
    expect(viewTitle(h, 3000, "earth")).toEqual({ title: "Germany", subtitle: "Europe", entity: 1 });
    expect(viewTitle(h, 20000, "earth").title).toBe("World");
    const v = rankHits([f(12, "areas", "Wien", "Admin 1", 64), f(13, "areas", "Austria", "Sovereign", 62)]);
    expect(viewTitle(v, 800, "earth")).toEqual({ title: "Wien", subtitle: "Austria", entity: 12 });
    expect(viewTitle(v, 3000, "earth").title).toBe("Austria");
    expect(viewTitle(h, 3000, "moon").title).toBe("Moon");
    expect(viewTitle(rankHits([f(3, "areas", "Atlantic Ocean", "Ocean")]), 3000, "earth").title).toBe("Atlantic Ocean");
  });
});
