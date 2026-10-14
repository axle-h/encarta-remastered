import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, parseSettings } from "./settings";

describe("settings", () => {
  it("falls back to defaults for missing or malformed storage", () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("{nope")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("[1]")).toEqual(DEFAULT_SETTINGS);
  });
  it("keeps valid values and drops invalid ones", () => {
    const s = parseSettings(JSON.stringify({ textSize: "large", units: "metric", latLongFormat: "bogus", captions: "yes", menuSounds: false }));
    expect(s).toMatchObject({ textSize: "large", units: "metric", latLongFormat: "dm", captions: false, menuSounds: false });
  });
});
