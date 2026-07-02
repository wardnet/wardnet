import { describe, expect, it } from "vitest";
import {
  isDeviceOnline,
  DEVICE_TYPE_OPTIONS,
  deviceTypeLabel,
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
