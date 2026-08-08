import { describe, expect, it } from "vitest";
import { SERVICE_OPTIONS, matchServiceLabel } from "@/lib/serviceBundles";

describe("serviceBundles", () => {
  it("offers the two real backend presets plus custom", () => {
    const presets = SERVICE_OPTIONS.filter(
      (o) => o.spec !== "custom" && o.spec.type === "preset",
    );
    expect(presets.map((p) => p.id)).toEqual(["casting", "smart-home"]);
    expect(SERVICE_OPTIONS[SERVICE_OPTIONS.length - 1].id).toBe("custom");
  });

  it("matches the casting preset back to its label", () => {
    expect(matchServiceLabel({ type: "preset", set: "casting" })).toMatch(
      /Casting/,
    );
  });

  it("matches the smart-home preset back to its label", () => {
    // Without an entry in SERVICE_OPTIONS this returns null and the card falls
    // back to a generic summary, which is what made the original bug so hard to
    // diagnose from the UI.
    expect(matchServiceLabel({ type: "preset", set: "smart_home" })).toMatch(
      /Smart home/,
    );
  });

  it("keeps HTTP/HTTPS out of the smart-home preset, in the web bundle", () => {
    // The preset is a backend one, so the UI carries no port list for it — the
    // separation that matters here is that `web` stays a distinct, deliberate
    // choice rather than being folded into the smart-home entry.
    const smartHome = SERVICE_OPTIONS.find((o) => o.id === "smart-home");
    expect(smartHome?.spec).toEqual({ type: "preset", set: "smart_home" });
    expect(SERVICE_OPTIONS.some((o) => o.id === "web")).toBe(true);
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
