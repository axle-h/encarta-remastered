import { describe, expect, it } from "vitest";
import { eventDateText, formatEventDate } from "./timelineDates";

describe("timeline dates of a topic row (no start)", () => {
  it("gives an empty date instead of throwing", () => {
    expect(formatEventDate({ start: null })).toBe("");
    expect(eventDateText({ start: null })).toBe("");
    expect(eventDateText({ start: null, date_text: "1789 - 1799" })).toBe("1789 - 1799");
  });
  it("still formats dated events", () => {
    expect(eventDateText({ start: { year: 1789, month: 7, day: 14 } })).toBe("July 14, 1789");
  });
});
