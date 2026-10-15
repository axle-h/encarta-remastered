import { describe, expect, it } from "vitest";
import { hitRank, hoverClassesAt, hoverTarget, labelHitAt, rankHits, viewTitle, type QueriedFeature } from "../hit";
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
  it("sorts by MOBB's rank, class scale, then GeoObj descending (mad-styling.md §4.5)", () => {
    // Rows: 10 town (scale 200), 20 region (250), 21 division (250), 30 range (400), 31 basin (420),
    // 32 country (420), 40 river line (400).
    const order = { scale: new Map([[10, 200], [20, 250], [21, 250], [30, 400], [31, 420], [32, 420], [40, 400]]), routable: new Set<number>() };
    const g = (key: number, entry: number) => ({ geoobj: key * 65536 + entry });
    const menu = rankHits(
      [
        f(1, "areas", "Country", "CountryRegion", 32, g(0, 169)),
        f(2, "areas", "Region B", "Statistical region", 20, g(8, 24)),
        f(3, "areas", "Basin", "River - basin", 31, g(60, 92)),
        f(4, "areas", "Range", "Physiographic region", 30, g(60, 59)),
        f(5, "areas", "Division", "Admin1", 21, g(208, 94)),
        f(6, "areas", "Region A", "Statistical region", 20, g(8, 25)),
        f(7, "lines", "River", "River", 40, g(16, 186)),
        f(8, "points", "Town", "PPL", 10, g(64, 35)),
      ],
      undefined,
      undefined,
      { order },
    );
    expect(menu.map((h) => h.name)).toEqual(["Town", "River", "Division", "Region A", "Region B", "Range", "Basin", "Country"]);
  });
  it("lists places hit through their drawn label first, a routable road's label before all", () => {
    const order = { scale: new Map([[10, 200], [32, 420], [50, 400], [60, 400]]), routable: new Set([60]) };
    const sym = (q: QueriedFeature): QueriedFeature => ({ ...q, layer: { id: "label", type: "symbol" } });
    const menu = rankHits(
      [
        f(1, "areas", "Sea", "Water label", 50),
        f(2, "points", "Town", "PPL", 10),
        sym(f(3, "labels", "Country", "CountryRegion", 32)),
        f(4, "lines", "Motorway", "Motorway", 60),
        sym(f(5, "lines", "Highway", "Highway", 60)),
      ],
      undefined,
      undefined,
      { order },
    );
    expect(menu.map((h) => h.name)).toEqual(["Highway", "Country", "Town", "Motorway", "Sea"]);
    expect(menu[1]!.label).toBe(true);
    expect(hitRank({ kind: "area", label: false, classId: 1 })).toBe(5);
    // At most four routable roads.
    const roads = rankHits([1, 2, 3, 4, 5, 6].map((i) => f(i, "lines", `Road ${i}`, "Motorway", 60)), undefined, undefined, { order });
    expect(roads.map((h) => h.name)).toEqual(["Road 1", "Road 2", "Road 3", "Road 4"]);
  });
  it("tells a point's label from its symbol by where the click is", () => {
    const pt = (icon: boolean): QueriedFeature => ({ ...f(1, "points", "Town", "PPL"), geometry: { type: "Point", coordinates: [icon ? 1 : 2, 0] } });
    const project = ([x, y]: [number, number]) => ({ x: x * 100, y });
    const radius = (q: QueriedFeature) => ((q.geometry?.coordinates as number[])[0] === 1 ? 4 : undefined);
    const test = labelHitAt({ x: 103, y: 0 }, project, radius);
    expect(test(pt(true))).toBe(false); // 3 px from a 4-px-radius icon
    expect(labelHitAt({ x: 112, y: 0 }, project, radius)(pt(true))).toBe(true); // beside it: the name
    expect(test(pt(false))).toBe(true); // a name without a symbol
    expect(test({ ...f(2, "areas", "Country", "CountryRegion"), layer: { id: "fill", type: "fill" } })).toBe(false);
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
