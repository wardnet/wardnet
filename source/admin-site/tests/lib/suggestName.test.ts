import { afterEach, describe, expect, it, vi } from "vitest";
import { isReservedName, isValidName, suggestName } from "@/lib/suggestName";

describe("isValidName", () => {
  it("rejects names shorter than 3 or longer than 32 chars", () => {
    expect(isValidName("ab")).toBe(false);
    expect(isValidName("a".repeat(33))).toBe(false);
    expect(isValidName("abc")).toBe(true);
  });

  it("rejects leading/trailing hyphens", () => {
    expect(isValidName("-abc")).toBe(false);
    expect(isValidName("abc-")).toBe(false);
    expect(isValidName("a-b")).toBe(true);
  });

  it("rejects disallowed characters", () => {
    expect(isValidName("Abc")).toBe(false);
    expect(isValidName("a_b")).toBe(false);
    expect(isValidName("a b")).toBe(false);
    expect(isValidName("ab1-x")).toBe(true);
  });

  it("rejects reserved labels", () => {
    expect(isValidName("admin")).toBe(false);
    expect(isValidName("wardnet")).toBe(false);
    expect(isValidName("happy-einstein")).toBe(true);
  });
});

describe("isReservedName", () => {
  it("flags reserved and passes non-reserved", () => {
    expect(isReservedName("api")).toBe(true);
    expect(isReservedName("not-reserved")).toBe(false);
  });
});

describe("suggestName", () => {
  const realRandom = Math.random;
  afterEach(() => {
    Math.random = realRandom;
  });

  it("produces an adjective-scientist slug that is valid", () => {
    const name = suggestName();
    expect(name).toMatch(/^[a-z]+-[a-z]+$/);
    expect(isValidName(name)).toBe(true);
  });

  it("picks the first entries when random is 0", () => {
    Math.random = vi.fn(() => 0) as unknown as typeof Math.random;
    expect(suggestName()).toBe("happy-einstein");
  });
});
