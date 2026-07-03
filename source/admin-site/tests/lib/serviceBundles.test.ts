import { describe, expect, it } from "vitest";
import { SERVICE_OPTIONS, matchServiceLabel } from "@/lib/serviceBundles";

describe("serviceBundles", () => {
  it("offers casting as the only real preset plus custom", () => {
    const presets = SERVICE_OPTIONS.filter(
      (o) => o.spec !== "custom" && o.spec.type === "preset",
    );
    expect(presets).toHaveLength(1);
    expect(presets[0].id).toBe("casting");
    expect(SERVICE_OPTIONS[SERVICE_OPTIONS.length - 1].id).toBe("custom");
  });

  it("matches the casting preset back to its label", () => {
    expect(matchServiceLabel({ type: "preset", set: "casting" })).toMatch(
      /Casting/,
    );
  });

  it("matches a known port bundle regardless of order", () => {
    expect(
      matchServiceLabel({
        type: "ports",
        ports: [{ proto: "tcp", from: 22, to: 22 }],
      }),
    ).toBe("SSH");
    // Web bundle, ports reversed.
    expect(
      matchServiceLabel({
        type: "ports",
        ports: [
          { proto: "tcp", from: 443, to: 443 },
          { proto: "tcp", from: 80, to: 80 },
        ],
      }),
    ).toMatch(/Web/);
  });

  it("returns null for an unknown custom port list", () => {
    expect(
      matchServiceLabel({
        type: "ports",
        ports: [{ proto: "udp", from: 12345, to: 12345 }],
      }),
    ).toBeNull();
  });
});
