import { describe, expect, it } from "vitest";
import { breakPoints, labelLayouts, layoutCost, splitLines, spacedCount } from "../labelText";
import { backgroundCost, candidateBox, joinRuns, placeLabels, placeShield, type Label, type LabelOrder, type SizedLayout, type TextLabel } from "../placement";
import { anchorFor, ClassGrid, evalFilter, overlayStyle } from "../labelOverlay";
import type { StyleSpecification } from "maplibre-gl";

describe("label lines (mad-styling.md §7.3)", () => {
  it("breaks at spaces and dashes, not at hyphen-minus, and keeps a dash", () => {
    expect(breakPoints("Saint-Malo de la Mer").breaks).toEqual([10, 13, 16]);
    expect(splitLines("Saint-Malo de la Mer", 2)).toEqual(["Saint-Malo", "de la Mer"]);
    expect(splitLines("Alpha–Beta Gamma", 2)).toEqual(["Alpha–Beta", "Gamma"]);
    expect(splitLines("Alpha–Beta", 2)).toEqual(["Alpha–", "Beta"]);
  });

  it("aims at an even share and refuses lines under 5 characters", () => {
    expect(splitLines("Caldaro sulla strada del vino", 3)).toEqual(["Caldaro", "sulla strada", "del vino"]);
    expect(splitLines("Caldaro sulla strada del vino", 2)).toEqual(["Caldaro sulla", "strada del vino"]);
    expect(splitLines("San Pancrazio", 2)).toEqual(["San", "Pancrazio"]);
    expect(splitLines("San Pancrazio", 3)).toBeNull(); // one break only
    expect(splitLines("Ab Cd Ef", 2)).toBeNull(); // 8 / 2 < 5
  });

  it("puts a bracketed tail on its own last line", () => {
    expect(breakPoints("Georgia (U.S. state)")).toEqual({ breaks: [], bracket: 8 });
    expect(splitLines("Georgia (U.S. state)", 2)).toEqual(["Georgia", "(U.S. state)"]);
    expect(splitLines("Georgia (U.S. state)", 1)).toEqual(["Georgia (U.S. state)"]);
  });

  it("costs about 12 characters per line", () => {
    expect(layoutCost(["Mumbai", "Bombay"])).toBe(250);
    expect(layoutCost(["San Pancrazio"])).toBe(500);
    expect(layoutCost(["San", "Pancrazio"])).toBe(500);
    expect(layoutCost(["Caldaro", "sulla strada", "del vino"])).toBe(0);
    expect(layoutCost(["Caldaro", "sulla strada", "del vino", "Kaltern an der Weinstrasse"])).toBe(750);
    expect(layoutCost(["Paris"])).toBe(0);
    expect(layoutCost(["Ur"])).toBe(500);
  });

  it("groups layouts: with the alternative, alone, abbreviated", () => {
    const g = labelLayouts({ main: "Mumbai", alt: "Bombay", abbr: "Mum." });
    expect(g.map((x) => x[0]!.group)).toEqual(["alt", "main", "abbr"]);
    expect(g[0]!.map((l) => [l.lines, l.alt])).toEqual([[["Mumbai"], "Bombay"]]);
    expect(labelLayouts({ main: "United Kingdom" })[0]!.map((l) => l.lines)).toEqual([["United Kingdom"], ["United", "Kingdom"]]);
  });

  it("leaves the letter spacing out before a full stop", () => {
    expect(spacedCount("St. Ives")).toBe(6);
  });
});

const order = (priority: number, entity: number): LabelOrder => ({ flagged: false, priority, icon: 65535, geoobj: entity, entity });
const sized = (lines: string[], w: number, h: number, cost = 0): SizedLayout => ({ group: "main", lines, cost, w, h });
const env = { width: 800, height: 500, inside: (b: { x0: number; y0: number; x1: number; y1: number }) => b.x0 >= 0 && b.y0 >= 0 && b.x1 <= 800 && b.y1 <= 500 };
const point = (x: number, y: number, entity: number, priority = 80, w = 40): TextLabel => ({
  kind: "point",
  x,
  y,
  symbol: 6,
  groups: [[sized(["Town"], w, 12)]],
  reserve: { w: 30, h: 12 },
  order: order(priority, entity),
});

describe("placement (mad-styling.md §7.4)", () => {
  it("puts an isolated label above right", () => {
    const [p] = placeLabels([point(200, 200, 1)], env);
    expect(p).toMatchObject({ pos: "above-right", icon: true });
    if (p?.kind === "point") expect(p.box).toEqual(candidateBox("above-right", 200, 200, 3, 40, 12));
  });

  it("weighs what lies under the box: water, land, coasts and lines", () => {
    // All land with a road at x = 250: the right-hand boxes cross it, so the label goes above left.
    const grid = new ClassGrid(800, 500, 4);
    grid.addSegment(250, 0, 250, 500, 1);
    grid.finish();
    const [p] = placeLabels([point(230, 200, 1)], { ...env, classes: (b) => grid.counts(b) });
    expect(p).toMatchObject({ pos: "above-left" });
    expect(backgroundCost({ water: 100, land: 0, marked: 0, total: 100 })).toBe(0);
    expect(backgroundCost({ water: 0, land: 100, marked: 0, total: 100 })).toBe(251);
    expect(backgroundCost({ water: 30, land: 70, marked: 0, total: 100 })).toBe(2000);
    expect(backgroundCost({ water: 0, land: 98, marked: 2, total: 100 })).toBe(251 + 2000);
  });

  it("places by priority across kinds and never overlaps", () => {
    const city = point(300, 200, 1, 23, 60);
    const town = point(310, 200, 2, 80, 60);
    const area: TextLabel = { kind: "area", x: 340, y: 194, symbol: 0, groups: [[sized(["REGION"], 50, 12)]], order: order(50, 3) };
    const out = placeLabels([town, area, city], env);
    expect(out.map((p) => p.label.order.entity)).toEqual([1, 3, 2].filter((e) => out.some((p) => p.label.order.entity === e)));
    const boxes = out.flatMap((p) => (p.kind !== "shield" && p.box ? [p.box] : []));
    for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) expect(boxes[i]!.x1 <= boxes[j]!.x0 || boxes[j]!.x1 <= boxes[i]!.x0 || boxes[i]!.y1 <= boxes[j]!.y0 || boxes[j]!.y1 <= boxes[i]!.y0).toBe(true);
  });

  it("falls back to the next layout group only when nothing of the first fits, and drops a point that fits nowhere", () => {
    const wide: TextLabel = { ...point(780, 250, 1), groups: [[{ ...sized(["Mumbai"], 50, 24), group: "alt", alt: "Bombay" }], [sized(["Mumbai"], 50, 12)]] };
    const [p] = placeLabels([wide], { ...env, inside: (b) => b.y0 >= 240 && b.x0 >= 0 && b.x1 <= 800 });
    expect(p).toMatchObject({ pos: "below-left" });
    expect(p?.kind === "point" && p.layout?.group).toBe("alt");
    const [q] = placeLabels([wide], { ...env, inside: (b) => b.y1 - b.y0 <= 12 && b.x0 >= 0 });
    expect(q?.kind === "point" && q.layout?.group).toBe("main");
    expect(placeLabels([wide], { ...env, inside: () => false })).toEqual([]);
  });

  it("puts one shield near the middle of a run, and none within ~1/6 of the pane of the same text", () => {
    const run = (y: number, entity: number): Label => ({ kind: "shield", line: [[100, y], [500, y]], w: 24, h: 14, text: "95", order: order(120, entity) });
    const s = placeShield(run(100, 1) as never, () => true, () => 0);
    expect(s && Math.round(s.x)).toBe(288); // 16 candidates from 24 to 376 along the run: two tie nearest 200, the first wins
    const both = placeLabels([run(100, 1), run(150, 2), run(400, 3)], env);
    expect(both.map((p) => p.label.order.entity)).toEqual([1, 3]);
  });

  it("joins tile pieces of a road into one run", () => {
    const runs = joinRuns([
      [
        [0, 0],
        [10, 0],
      ],
      [
        [20, 0],
        [10, 0],
      ],
      [
        [50, 50],
        [60, 50],
      ],
    ]);
    expect(runs.length).toBe(2);
    expect(runs.find((r) => r.length === 3)).toBeTruthy();
  });
});

describe("overlay layers (labelOverlay.ts)", () => {
  const style = {
    version: 8,
    glyphs: "/g/{fontstack}/{range}.pbf",
    sources: { earth: { type: "vector", url: "x" } },
    layers: [
      { id: "fill", type: "fill", source: "earth", "source-layer": "areas" },
      {
        id: "town",
        type: "symbol",
        source: "earth",
        "source-layer": "points",
        filter: ["==", ["get", "class"], 7],
        layout: {
          "icon-image": "bm10",
          "text-font": ["Arial Regular"],
          "text-size": 12,
          "text-variable-anchor": ["bottom-left"],
          "text-field": [
            "case",
            ["has", "alt_names"],
            ["format", ["coalesce", ["get", "name"], ""], { "text-font": ["literal", ["Arial Regular Rule 24.00 2.00 0.00"]] }, "\n", {}, ["get", "alt_names"], { "text-font": ["literal", ["Arial Regular Rule -4.80 2.40 0.00"]], "font-scale": 0.8, "text-color": "#000000" }],
            ["coalesce", ["get", "name"], ""],
          ],
        },
      },
      { id: "river", type: "symbol", source: "earth", "source-layer": "lines", layout: { "text-field": ["get", "name"], "symbol-placement": "line" } },
      { id: "shield", type: "symbol", source: "earth", "source-layer": "lines", layout: { "text-field": ["upcase", ["coalesce", ["get", "name"], ""]], "text-font": ["Arial Bold"], "icon-image": "shield1", "icon-text-fit": "both", "symbol-sort-key": 120 } },
    ],
  } as unknown as StyleSpecification;

  it("moves the labels onto the placement's source and line names under them", () => {
    const { style: out, specs } = overlayStyle(style);
    expect(out.layers.map((l) => l.id)).toEqual(["fill", "river", "town", "shield"]);
    expect(specs.map((s) => [s.id, s.kind])).toEqual([
      ["town", "point"],
      ["shield", "shield"],
    ]);
    expect(specs[0]!.alt).toEqual({ mainRuleFont: "Arial Regular Rule 24.00 2.00 0.00", font: "Arial Regular Rule -4.80 2.40 0.00", scale: 0.8, upper: false });
    expect(specs[1]!).toMatchObject({ upper: true, priority: 120 });
    const town = out.layers[2] as { source: string; filter: unknown; layout: Record<string, unknown> };
    expect(town.source).toBe("encarta-labels");
    expect(town.filter).toEqual(["==", ["get", "_layer"], "town"]);
    expect(town.layout["text-variable-anchor"]).toBeUndefined();
    expect(town.layout["text-allow-overlap"]).toBe(true);
  });

  it("evaluates the converted filters and anchors labels as placed", () => {
    expect(evalFilter(["all", ["in", ["get", "class"], ["literal", [1, 2]]], ["has", "name"]], { class: 2, name: "x" })).toBe(true);
    expect(evalFilter(["==", ["get", "class"], 7], { class: 8 })).toBe(false);
    expect(anchorFor("below-left", 3, 12, false)).toEqual({ anchor: "top-right", d: 0.25, justify: "right" });
    expect(anchorFor("above-right", 3, 12, true).justify).toBe("center");
  });
});
