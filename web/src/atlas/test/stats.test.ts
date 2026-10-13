import { describe, expect, it } from "vitest";
import { colourValues, statColour, statRamp } from "../stats";
import { syntheticStyle } from "./fixtures";

describe("statistics colours", () => {
  const ramp = statRamp(syntheticStyle());
  it("reads the STAT ramp from the style", () => {
    expect(ramp).toEqual({ positive: ["#fbe3c6", "#bd3d1f"], negative: ["#7354b2"] });
  });
  it("maps values from 0 to the extreme", () => {
    expect(statColour(0, ramp, 10, 0)).toBe("#fbe3c6");
    expect(statColour(4.9, ramp, 10, 0)).toBe("#fbe3c6");
    expect(statColour(5, ramp, 10, 0)).toBe("#bd3d1f");
    expect(statColour(10, ramp, 10, 0)).toBe("#bd3d1f");
    expect(statColour(-3, ramp, 10, -3)).toBe("#7354b2");
    expect(statColour(NaN, ramp, 10, 0)).toBeNull();
  });
  it("colours countries, skipping missing values", () => {
    const m = colourValues([{ country_id: 1, value: 1 }, { country_id: 2, value: 100 }, { country_id: 3, value: null }], ramp);
    expect([...m.entries()]).toEqual([[1, "#fbe3c6"], [2, "#bd3d1f"]]);
  });
});
