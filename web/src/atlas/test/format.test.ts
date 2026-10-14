import { describe, expect, it } from "vitest";
import { formatArea, formatCoord, formatDistance, formatLatLong } from "../format";
import { colourValues, statColour } from "../stats";
import { formatAltitude } from "../zoom";

describe("lat/long formats", () => {
  it("formats degrees and minutes, with seconds, and decimal", () => {
    expect(formatLatLong(12.5, -45.25, "dm")).toBe("12°30′ N, 45°15′ W");
    expect(formatLatLong(-12.5042, 45.2542, "dms")).toBe("12°30′15″ S, 45°15′15″ E");
    expect(formatLatLong(12.5, -45.25, "decimal")).toBe("12.5000° N, 45.2500° W");
  });
  it("carries a rounded-up minute", () => {
    expect(formatCoord(9.9999, "lat", "dm")).toBe("10°00′ N");
    expect(formatCoord(9.99999, "lon", "dms")).toBe("10°00′00″ E");
  });
});

describe("units", () => {
  it("converts distances and areas", () => {
    expect(formatDistance(160.9344, "us")).toBe("100 mi");
    expect(formatDistance(5, "metric")).toBe("5.0 km");
    expect(formatArea(2.589988, "us")).toBe("1 sq mi");
    expect(formatArea(1000, "metric")).toBe("1,000 sq km");
    expect(formatAltitude(1609.344, "us")).toBe("1,000 mi");
    expect(formatAltitude(1609.344)).toBe("1,609 km");
  });
});

describe("linear statistics", () => {
  const ramp = { positive: ["#000000", "#808080", "#ffffff"], negative: [] };
  it("blends continuously instead of picking a class", () => {
    expect(statColour(5, ramp, 10, 0)).toBe("#808080");
    expect(statColour(2.5, ramp, 10, 0, true)).toBe("#404040");
    expect(colourValues([{ country_id: 1, value: 10 }], ramp, true).get(1)).toBe("#ffffff");
  });
});
