import { describe, expect, it } from "vitest";
import { classGroups, classShown, groupOf, setShown } from "../groups";
import { cssFont, legendEntries, prettyClassName } from "../legend";
import { syntheticStyle } from "./fixtures";

const none = { on: new Set<number>(), off: new Set<number>() };

describe("map key", () => {
  it("lists what is drawn at this zoom, points first", () => {
    const e = legendEntries(syntheticStyle(), 4, none);
    expect(e.map((x) => [x.name, x.kind])).toEqual([
      ["PPL - non-capital million+ city", "point"],
      ["Sovereign - boundary", "line"],
      ["River", "line"],
      ["Railroad - minor", "line"],
      ["CountryRegion - Yellow", "area"],
      ["Sovereign", "label"],
    ]);
    expect(e[0]!.icon).toBe("bm10");
    expect(e[1]!.lines[0]).toEqual({ color: "#000000", width: 1, dash: [7, 5], offset: undefined });
  });
  it("drops classes the user turned off and the STAT classes unless statistics are on", () => {
    const e = legendEntries(syntheticStyle(), 2, { on: new Set(), off: new Set([80]) });
    expect(e.find((x) => x.name === "River")).toBeUndefined();
    expect(e.find((x) => x.name === "Sovereign - boundary")).toBeUndefined(); // minzoom 3
    expect(legendEntries(syntheticStyle(), 2, none, { stats: true }).some((x) => x.name.startsWith("Positive"))).toBe(true);
  });
  it("prettifies names and fonts", () => {
    expect(prettyClassName("LANG - Indo-European")).toBe("Indo-European");
    expect(prettyClassName("Z - (STAT) Positive 1")).toBe("Positive 1");
    expect(cssFont("MS Reference Serif Bold Italic", 12)).toBe('italic bold 12px "MS Reference Serif", serif');
    expect(cssFont("Arial Regular")).toBe('11px "Arial", sans-serif');
  });
});

describe("customizer groups", () => {
  it("groups classes by name", () => {
    expect(groupOf("PPL - non-capital million+ city")).toBe("places");
    expect(groupOf("Sovereign - boundary")).toBe("borders");
    expect(groupOf("Railroad - minor")).toBe("transport");
    expect(groupOf("River")).toBe("water");
    expect(groupOf("Something else")).toBe("other");
    const g = classGroups(syntheticStyle());
    expect(g.map((x) => x.key)).toEqual(["places", "borders", "transport", "water", "regions"]);
    const transport = g.find((x) => x.key === "transport")!;
    expect(transport.classes.map((c) => c.name)).toEqual(["Airport", "Railroad - minor"]);
  });
  it("toggles default and optional classes the MOBB way", () => {
    const items = classGroups(syntheticStyle()).find((x) => x.key === "transport")!.classes;
    const airport = items.find((c) => c.name === "Airport")!;
    expect(classShown(airport, new Set(), new Set())).toBe(false);
    const on = setShown(items, true, new Set(), new Set());
    expect([...on.on]).toEqual([7]);
    expect(on.off.size).toBe(0);
    const off = setShown(items, false, on.on, on.off);
    expect([...off.off].sort()).toEqual([7, 81]);
    expect(off.on.size).toBe(0);
  });
});
