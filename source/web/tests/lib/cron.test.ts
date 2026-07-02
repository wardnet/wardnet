import { describe, expect, it } from "vitest";
import { cronToHuman } from "../../src/lib/cron";

describe("cronToHuman", () => {
  it("describes a valid cron expression in plain English", () => {
    const result = cronToHuman("0 3 * * *");
    expect(result.toLowerCase()).toContain("at");
    expect(result).not.toBe("0 3 * * *");
  });
  it("returns the raw expression when it can't be parsed", () => {
    expect(cronToHuman("not a cron")).toBe("not a cron");
  });
});
