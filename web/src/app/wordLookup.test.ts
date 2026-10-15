import { describe, expect, it } from "vitest";
import { lookupWord } from "./wordLookup";

describe("lookupWord", () => {
  it("trims punctuation and spaces", () => {
    expect(lookupWord("expanses, ")).toBe("expanses");
    expect(lookupWord("(Paris)")).toBe("Paris");
  });
  it("refuses empty or multi-word selections", () => {
    expect(lookupWord(" . ")).toBe("");
    expect(lookupWord("two words")).toBe("");
  });
});
