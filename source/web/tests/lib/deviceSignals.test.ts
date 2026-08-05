import { describe, expect, it } from "vitest";
import type { DeviceSignal, DeviceSignalKind } from "@wardnet/js";
import { groupSignalsByKind, signalKindDisplay } from "../../src/lib/deviceSignals";

function signal(
  kind: DeviceSignalKind,
  value: string,
  inferred = false,
): DeviceSignal {
  return { kind, value, inferred, observed_at: "2026-08-05T10:00:00Z" };
}

describe("signalKindDisplay", () => {
  it("labels and explains every known kind", () => {
    const kinds: DeviceSignalKind[] = [
      "dhcp_hostname",
      "dhcp_param_list",
      "dhcp_vendor_class",
      "mdns_service",
      "probed_port",
    ];
    for (const kind of kinds) {
      const display = signalKindDisplay(kind);
      expect(display.label).not.toBe(kind);
      expect(display.hint.length).toBeGreaterThan(0);
    }
  });

  it("falls back to the raw kind for one a newer daemon wrote", () => {
    const display = signalKindDisplay("ssdp_service" as DeviceSignalKind);
    expect(display.label).toBe("ssdp_service");
    expect(display.hint.length).toBeGreaterThan(0);
  });
});

describe("groupSignalsByKind", () => {
  it("returns nothing for no signals", () => {
    expect(groupSignalsByKind([])).toEqual([]);
  });

  it("groups by kind and preserves the server's order within a group", () => {
    const groups = groupSignalsByKind([
      signal("mdns_service", "_govee._tcp"),
      signal("mdns_service", "_hap._tcp"),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].signals.map((s) => s.value)).toEqual([
      "_govee._tcp",
      "_hap._tcp",
    ]);
  });

  it("orders vendor-bearing kinds ahead of the device-class fingerprint", () => {
    const groups = groupSignalsByKind([
      signal("dhcp_param_list", "1,3,6,15"),
      signal("dhcp_hostname", "govee-lamp"),
      signal("mdns_service", "_govee._tcp"),
    ]);
    expect(groups.map((g) => g.kind)).toEqual([
      "mdns_service",
      "dhcp_hostname",
      "dhcp_param_list",
    ]);
  });

  it("keeps an unrecognised kind rather than dropping the evidence", () => {
    const groups = groupSignalsByKind([
      signal("ssdp_service" as DeviceSignalKind, "urn:roku:device"),
      signal("mdns_service", "_govee._tcp"),
    ]);
    // Known kinds lead; the unknown one trails but survives.
    expect(groups.map((g) => g.kind)).toEqual(["mdns_service", "ssdp_service"]);
  });

  it("carries the inferred flag through to the group", () => {
    const groups = groupSignalsByKind([
      signal("dhcp_vendor_class", "Govee", true),
    ]);
    expect(groups[0].signals[0].inferred).toBe(true);
  });
});
