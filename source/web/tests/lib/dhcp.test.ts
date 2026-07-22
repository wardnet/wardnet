import { describe, expect, it } from "vitest";
import { poolUsagePercent } from "../../src/lib/dhcp";

describe("poolUsagePercent", () => {
  it("computes a rounded percentage for a normal pool", () => {
    expect(poolUsagePercent(25, 100)).toBe(25);
    expect(poolUsagePercent(1, 3)).toBe(33);
  });

  it("clamps usage above the pool size to 100", () => {
    // Shrinking the pool below the active-lease count is a handled scenario;
    // it must not report >100% next to a bar capped at 100%.
    expect(poolUsagePercent(120, 100)).toBe(100);
  });

  it("floors negative usage at 0", () => {
    expect(poolUsagePercent(-5, 100)).toBe(0);
  });

  it("treats a non-positive pool as 0% without dividing by zero", () => {
    expect(poolUsagePercent(0, 0)).toBe(0);
    expect(poolUsagePercent(5, 0)).toBe(0);
  });
});
