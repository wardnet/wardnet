import { describe, expect, it } from "vitest";
import { resolveIcon } from "@/lib/icons";

describe("resolveIcon", () => {
  it("returns a component for a known icon name", () => {
    expect(resolveIcon("download")).toBeDefined();
    expect(resolveIcon("route")).toBeDefined();
    expect(resolveIcon("shield")).toBeDefined();
    expect(resolveIcon("code")).toBeDefined();
  });

  it("returns undefined for an unknown icon name", () => {
    expect(resolveIcon("nonexistent")).toBeUndefined();
  });
});
