import { describe, expect, it } from "vitest";
import type { Device } from "@wardnet/js";
import { suggestHostnameForMac } from "../../src/lib/utils";

type PartialDevice = Pick<Device, "name" | "hostname" | "mac">;

function device(overrides: Partial<PartialDevice>): PartialDevice {
  return {
    mac: "AA:BB:CC:DD:EE:FF",
    name: null,
    hostname: null,
    ...overrides,
  };
}

describe("suggestHostnameForMac", () => {
  it("returns the device name for a unique match", () => {
    const devices = [
      device({ mac: "AA:BB:CC:DD:EE:FF", name: "Office printer" }),
    ];
    expect(suggestHostnameForMac(devices, "AA:BB:CC:DD:EE:FF")).toBe(
      "Office printer",
    );
  });

  it("falls back to the hostname when the device has no name", () => {
    const devices = [
      device({ mac: "AA:BB:CC:DD:EE:FF", name: null, hostname: "printer-1" }),
    ];
    expect(suggestHostnameForMac(devices, "AA:BB:CC:DD:EE:FF")).toBe(
      "printer-1",
    );
  });

  it("matches across formats and casing", () => {
    const devices = [device({ mac: "aa-bb-cc-dd-ee-ff", name: "TV" })];
    // lowercase, no separators, colon-form all resolve to the same device.
    expect(suggestHostnameForMac(devices, "aabbccddeeff")).toBe("TV");
    expect(suggestHostnameForMac(devices, "AA:BB:CC:DD:EE:FF")).toBe("TV");
  });

  it("returns undefined when the MAC is incomplete", () => {
    const devices = [device({ mac: "AA:BB:CC:DD:EE:FF", name: "TV" })];
    expect(suggestHostnameForMac(devices, "AA:BB:CC")).toBeUndefined();
    expect(suggestHostnameForMac(devices, "")).toBeUndefined();
  });

  it("returns undefined when no device matches", () => {
    const devices = [device({ mac: "AA:BB:CC:DD:EE:FF", name: "TV" })];
    expect(suggestHostnameForMac(devices, "11:22:33:44:55:66")).toBeUndefined();
  });

  it("returns undefined when more than one device matches the MAC", () => {
    const devices = [
      device({ mac: "AA:BB:CC:DD:EE:FF", name: "First" }),
      device({ mac: "aa:bb:cc:dd:ee:ff", name: "Second" }),
    ];
    expect(suggestHostnameForMac(devices, "AA:BB:CC:DD:EE:FF")).toBeUndefined();
  });

  it("skips the suggestion when the display name is just the MAC", () => {
    // No name and no hostname -> deviceDisplayName falls back to the MAC,
    // which is useless as a hostname suggestion.
    const devices = [
      device({ mac: "AA:BB:CC:DD:EE:FF", name: null, hostname: null }),
    ];
    expect(suggestHostnameForMac(devices, "AA:BB:CC:DD:EE:FF")).toBeUndefined();
  });

  it("returns undefined for an empty device list", () => {
    expect(suggestHostnameForMac([], "AA:BB:CC:DD:EE:FF")).toBeUndefined();
  });
});
