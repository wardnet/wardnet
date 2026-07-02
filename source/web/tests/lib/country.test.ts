import { describe, expect, it } from "vitest";
import { countryFlag } from "../../src/lib/country";

describe("countryFlag", () => {
  it("converts a two-letter code to its regional-indicator emoji", () => {
    expect(countryFlag("US")).toBe("🇺🇸");
    expect(countryFlag("gb")).toBe("🇬🇧"); // lowercase is upper-cased first
  });
  it("returns an empty string when the code isn't exactly two chars", () => {
    expect(countryFlag("USA")).toBe("");
    expect(countryFlag("U")).toBe("");
    expect(countryFlag("")).toBe("");
  });
});
