import { describe, expect, it } from "vitest";
import type { Device } from "@wardnet/js";
import {
  isDeviceOnline,
  DEVICE_TYPE_OPTIONS,
  deviceTypeLabel,
  manufacturerDisplay,
} from "../../src/lib/device";

describe("isDeviceOnline", () => {
  it("is true when seen within the last 5 minutes", () => {
    expect(isDeviceOnline(new Date(Date.now() - 60_000).toISOString())).toBe(
      true,
    );
  });
  it("is false when seen longer ago than the threshold", () => {
    expect(
      isDeviceOnline(new Date(Date.now() - 6 * 60_000).toISOString()),
    ).toBe(false);
  });
  it("is false for an unparseable timestamp", () => {
    expect(isDeviceOnline("not a date")).toBe(false);
  });
});

describe("deviceTypeLabel", () => {
  it("returns the label for a known type", () => {
    expect(deviceTypeLabel("tv")).toBe("TV");
    expect(deviceTypeLabel("game_console")).toBe("Console");
  });
  it("falls back to the raw value for an unknown type", () => {
    // @ts-expect-error deliberately passing an unmapped value
    expect(deviceTypeLabel("mystery")).toBe("mystery");
  });
  it("exposes an options list covering every mapped type", () => {
    expect(DEVICE_TYPE_OPTIONS.length).toBeGreaterThan(5);
    expect(DEVICE_TYPE_OPTIONS.every((o) => o.value && o.label)).toBe(true);
  });
});

describe("manufacturerDisplay", () => {
  /** Minimal fixture — only the fields the helper reads. */
  const device = (o: Partial<Device>): Device => o as Device;

  it("states an IEEE registrant plainly, with nothing to explain", () => {
    const d = manufacturerDisplay(
      device({ manufacturer: "Apple Inc.", manufacturer_source: "ieee" }),
    );
    expect(d.label).toBe("Apple Inc.");
    expect(d.hint).toBeNull();
    expect(d.unknown).toBe(false);
  });

  it("hedges a curated catalog guess", () => {
    // The IEEE listing for these blocks is deliberately `Private`, so the name
    // is our inference and must not read as a registered fact (issue #1099).
    const d = manufacturerDisplay(
      device({ manufacturer: "Govee", manufacturer_source: "catalog" }),
    );
    expect(d.label).toBe("Likely Govee");
    expect(d.hint).toMatch(/informed guess/i);
    expect(d.unknown).toBe(false);
  });

  it("explains a name inferred from an observed signal", () => {
    const d = manufacturerDisplay(
      device({ manufacturer: "Google", manufacturer_source: "signal" }),
    );
    expect(d.label).toBe("Google");
    expect(d.hint).toMatch(/announced on the network/i);
  });

  it("never renders a bare dash when nothing is known", () => {
    // A blank left the admin unable to tell a Wardnet gap from a vendor who
    // chose a private listing — the dead end reported in the issue.
    const d = manufacturerDisplay(
      device({ manufacturer: null, manufacturer_source: null }),
    );
    expect(d.label).toBe("Unknown manufacturer");
    expect(d.unknown).toBe(true);
    expect(d.hint).toMatch(/private IEEE listing/i);
  });

  it("gives a randomized MAC its own reason for being unknown", () => {
    const d = manufacturerDisplay(
      device({ manufacturer: null, is_randomized: true }),
    );
    expect(d.label).toBe("Unknown manufacturer");
    expect(d.hint).toMatch(/randomized/i);
    // Distinct from the generic explanation, so the two cases are tellable.
    expect(d.hint).not.toMatch(/private IEEE listing/i);
  });
});
