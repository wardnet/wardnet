import { describe, expect, it } from "vitest";
import type { Device } from "@wardnet/js";
import {
  findNeighbourMacs,
  matchesDevice,
  normalizeMac,
} from "../../src/lib/deviceSearch";

/** Minimal device fixture — only the fields the search actually reads. */
function device(overrides: Partial<Device> & { mac: string }): Device {
  return {
    id: overrides.mac,
    name: null,
    hostname: null,
    manufacturer: null,
    last_ip: null,
    ...overrides,
  } as Device;
}

describe("normalizeMac", () => {
  it("collapses every separator convention to the same bare hex", () => {
    // The three formats an admin might paste from a vendor app, a router UI
    // or a spec sheet must all compare equal (issue #1099).
    expect(normalizeMac("5c:e7:53:4e:ec:db")).toBe("5ce7534eecdb");
    expect(normalizeMac("5C-E7-53-4E-EC-DB")).toBe("5ce7534eecdb");
    expect(normalizeMac("5CE7534EECDB")).toBe("5ce7534eecdb");
    expect(normalizeMac("5ce7.534e.ecdb")).toBe("5ce7534eecdb");
  });
});

describe("matchesDevice", () => {
  const govee = device({
    mac: "5c:e7:53:4e:ec:db",
    hostname: "govee-lamp",
    manufacturer: "Govee",
    last_ip: "192.168.100.51",
    name: "Living room lamp",
  });

  it("matches an empty query so an untouched search box hides nothing", () => {
    expect(matchesDevice(govee, "")).toBe(true);
    expect(matchesDevice(govee, "   ")).toBe(true);
  });

  it("matches name, hostname, manufacturer and IP", () => {
    expect(matchesDevice(govee, "living room")).toBe(true);
    expect(matchesDevice(govee, "govee-lamp")).toBe(true);
    expect(matchesDevice(govee, "Govee")).toBe(true);
    expect(matchesDevice(govee, "192.168.100.51")).toBe(true);
  });

  it("matches a full MAC regardless of the format typed", () => {
    expect(matchesDevice(govee, "5CE7534EECDB")).toBe(true);
    expect(matchesDevice(govee, "5c-e7-53-4e-ec-db")).toBe(true);
    expect(matchesDevice(govee, "5c:e7:53:4e:ec:db")).toBe(true);
  });

  it("matches on a bare OUI prefix so a vendor block can be listed", () => {
    expect(matchesDevice(govee, "5c:e7:53")).toBe(true);
    expect(matchesDevice(govee, "5ce753")).toBe(true);
  });

  it("does not match an unrelated MAC", () => {
    expect(matchesDevice(govee, "aa:bb:cc:dd:ee:ff")).toBe(false);
  });

  it("does not prefix-match MACs when the query is an IP", () => {
    // normalizeMac strips dots, so "10.0.0.1" would collapse to "10001", pass
    // the hex test, and prefix-match this MAC. An IP search must not surface
    // unrelated devices.
    const d = device({ mac: "10:00:1a:bb:cc:dd", last_ip: "192.168.1.9" });
    expect(matchesDevice(d, "10.0.0.1")).toBe(false);
    // A partial IP behaves the same way.
    expect(matchesDevice(d, "10.0")).toBe(false);
    // ...but a genuine IP match still works.
    expect(matchesDevice(d, "192.168.1.9")).toBe(true);
  });

  it("matches the tail of a MAC, which is what device labels print", () => {
    // Regression guard: this used to be a plain `includes` on the MAC. Making
    // it prefix-only silently broke searching the last few digits off a label,
    // and the new empty state made that miss look meaningful (issue #1099).
    expect(matchesDevice(govee, "ecdb")).toBe(true);
    expect(matchesDevice(govee, "ec:db")).toBe(true);
    expect(matchesDevice(govee, "4eecdb")).toBe(true);
  });

  it("requires a hex fragment to be long enough to mean something", () => {
    // Two digits appear inside a large share of addresses, so an unanchored
    // match that short is noise rather than a search. A prefix of any length
    // still matches — that is what OUI search relies on.
    expect(matchesDevice(govee, "ec")).toBe(false);
    expect(matchesDevice(govee, "5c")).toBe(true); // prefix, so allowed
  });

  it("does not treat a short non-hex query as a MAC prefix", () => {
    // "z" is not hex, so it must fall through to the text fields and miss —
    // rather than being normalised into a prefix that matches everything.
    expect(matchesDevice(device({ mac: "aa:bb:cc:dd:ee:ff" }), "z")).toBe(
      false,
    );
  });
});

// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
describe("findNeighbourMacs", () => {
  // Espressif derives Wi-Fi STA / Wi-Fi AP / BT / ETH from one base address as
  // base+0/+1/+2/+3, which is why a vendor app's Bluetooth MAC lands a few
  // away from the Wi-Fi MAC Wardnet actually saw.
  const wifi = device({ mac: "5c:e7:53:4e:ec:d9" });
  const far = device({ mac: "5c:e7:53:4e:ec:00" });

  it("surfaces a device a few addresses away from the searched MAC", () => {
    const hits = findNeighbourMacs([wifi, far], "5c:e7:53:4e:ec:db");
    expect(hits).toHaveLength(1);
    expect(hits[0].device.mac).toBe("5c:e7:53:4e:ec:d9");
    expect(hits[0].offset).toBe(-2);
  });

  it("excludes an exact match, which is a hit rather than a guess", () => {
    expect(findNeighbourMacs([wifi], "5c:e7:53:4e:ec:d9")).toEqual([]);
  });

  it("returns nothing for a partial MAC, which has no neighbourhood", () => {
    expect(findNeighbourMacs([wifi], "5c:e7:53")).toEqual([]);
    expect(findNeighbourMacs([wifi], "")).toEqual([]);
  });

  it("computes distance across the octet boundary, not within one octet", () => {
    // aa:...:00 and aa:...:fe are 2 apart as 48-bit values. Subtracting only
    // the last octet would score them 254 apart and miss the pair entirely.
    const wrapped = device({ mac: "aa:bb:cc:dd:ee:fe" });
    const hits = findNeighbourMacs([wrapped], "aa:bb:cc:dd:ef:00");
    expect(hits).toHaveLength(1);
    expect(hits[0].offset).toBe(-2);
  });

  it("does not pair two addresses that merely share a last octet", () => {
    // Same trailing byte, wildly different device. Last-octet arithmetic would
    // score these as identical; 48-bit arithmetic correctly rejects them.
    const unrelated = device({ mac: "aa:bb:cc:dd:ee:db" });
    expect(findNeighbourMacs([unrelated], "5c:e7:53:4e:ec:db")).toEqual([]);
  });

  it("sorts candidates nearest-first", () => {
    const plus1 = device({ mac: "5c:e7:53:4e:ec:dc" });
    const minus3 = device({ mac: "5c:e7:53:4e:ec:d8" });
    const hits = findNeighbourMacs([minus3, plus1], "5c:e7:53:4e:ec:db");
    expect(hits.map((h) => h.offset)).toEqual([1, -3]);
  });
});
