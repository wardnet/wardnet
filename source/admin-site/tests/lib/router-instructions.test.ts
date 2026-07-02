import { describe, expect, it } from "vitest";
import {
  ROUTER_INSTRUCTIONS,
  findRouterInstruction,
} from "@/lib/router-instructions";

describe("ROUTER_INSTRUCTIONS", () => {
  it("has unique kebab-case ids and non-empty steps", () => {
    const ids = ROUTER_INSTRUCTIONS.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const r of ROUTER_INSTRUCTIONS) {
      expect(r.id).toMatch(/^[a-z0-9-]+$/);
      expect(r.name.length).toBeGreaterThan(0);
      expect(r.steps.length).toBeGreaterThan(0);
    }
  });

  it("includes the 'other' sentinel", () => {
    expect(ROUTER_INSTRUCTIONS.some((r) => r.id === "other")).toBe(true);
  });
});

describe("findRouterInstruction", () => {
  it("returns the matching entry", () => {
    const found = findRouterInstruction("bt-smart-hub");
    expect(found?.name).toContain("BT");
  });

  it("returns undefined for an unknown id", () => {
    expect(findRouterInstruction("nope")).toBeUndefined();
  });
});
