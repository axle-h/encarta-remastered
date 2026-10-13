import { describe, expect, it } from "vitest";
import { altitudeForZoom, encartaAltitude, formatAltitude, insideBbox, maxZoomAt, parseMsEncartaUrl, zoomForAltitude } from "../zoom";

describe("altitude ↔ zoom (converter mapping, P/m = 2400)", () => {
  it("matches the band table of atlas-output.md", () => {
    expect(zoomForAltitude(150)).toBeCloseTo(8.97, 2);
    expect(zoomForAltitude(4000)).toBeCloseTo(4.23, 2);
    expect(zoomForAltitude(50000)).toBeCloseTo(0.59, 2);
    expect(zoomForAltitude(30)).toBeCloseTo(11.29, 2);
  });
  it("round-trips", () => {
    for (const a of [1, 5, 150, 3000, 14000, 50000]) expect(altitudeForZoom(zoomForAltitude(a))).toBeCloseTo(a, 6);
    expect(zoomForAltitude(3000, 4800)).toBeCloseTo(zoomForAltitude(3000) + 1, 9);
  });
  it("floors like MOBB and formats", () => {
    expect(encartaAltitude(zoomForAltitude(2999.6))).toBe(2999);
    expect(formatAltitude(3000)).toBe("3,000 km");
    expect(formatAltitude(12.34)).toBe("12 km");
    expect(formatAltitude(5)).toBe("5.0 km");
  });
});

describe("MinRect limits", () => {
  const rects = [{ bbox: [12.2, 45.3, 12.5, 45.5] as [number, number, number, number], maxzoom: 13.88 }];
  it("raises the max zoom only inside a box", () => {
    expect(maxZoomAt(12.33, 45.44, 8.97, rects)).toBe(13.88);
    expect(maxZoomAt(-69.95, 12.5, 8.97, rects)).toBe(8.97);
    expect(maxZoomAt(0, 0, 8.97, undefined)).toBe(8.97);
  });
  it("handles boxes across the antimeridian and longitudes beyond ±180", () => {
    expect(insideBbox(179.5, 0, [179, -1, -179, 1])).toBe(true);
    expect(insideBbox(-179.5, 0, [179, -1, -179, 1])).toBe(true);
    expect(insideBbox(0, 0, [179, -1, -179, 1])).toBe(false);
    expect(insideBbox(12.33 + 360, 45.44, [12.2, 45.3, 12.5, 45.5])).toBe(true);
  });
});

describe("msencarta:// map URLs", () => {
  it("reads lat/long/alt/style and eid", () => {
    expect(parseMsEncartaUrl("msencarta://view=MapView/cmd=map/lat=50.00/long=10.00/alt=3000.00/style=1")).toEqual({ lat: 50, lon: 10, altitudeKm: 3000, menuStyle: 1 });
    expect(parseMsEncartaUrl("msencarta://view=MapView/cmd=map/eid=25262/style=0")).toEqual({ entity: 25262, menuStyle: 0 });
    expect(parseMsEncartaUrl("https://example.org")).toBeNull();
  });
});

describe("flat view scale (mad-styling.md §2.1: half the globe's pixels per km)", () => {
  it("is one zoom level further out at the same altitude", async () => {
    const { zoomForView, altitudeForView } = await import("../zoom");
    expect(zoomForView(3000, true)).toBeCloseTo(zoomForView(3000, false) - 1, 9);
    expect(altitudeForView(zoomForView(1234, true), true)).toBeCloseTo(1234, 6);
  });
});
