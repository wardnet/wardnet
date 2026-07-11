import { describe, expect, it } from "vitest";
import {
  buildCidr,
  isPrivateCidr,
  isValidCidr,
  networkOctets,
  octetsPrivate,
  octetsValid,
  parseCidr,
  prefixForHosts,
  usableHosts,
} from "../../src/lib/cidr";

describe("cidr helpers", () => {
  it("validates octets", () => {
    expect(octetsValid([10, 44, 0, 0])).toBe(true);
    expect(octetsValid([10, 44, 0, 256])).toBe(false);
    expect(octetsValid([10, 44, 0])).toBe(false);
    expect(octetsValid([10, -1, 0, 0])).toBe(false);
  });

  it("computes usable hosts", () => {
    expect(usableHosts(24)).toBe(254);
    expect(usableHosts(26)).toBe(62);
    expect(usableHosts(30)).toBe(2);
    expect(usableHosts(31)).toBe(0);
    expect(usableHosts(32)).toBe(0);
  });

  it("picks the smallest network that fits a host count", () => {
    expect(prefixForHosts(50)).toBe(26); // 62 usable
    expect(prefixForHosts(62)).toBe(26);
    expect(prefixForHosts(63)).toBe(25); // 126 usable
    expect(prefixForHosts(254)).toBe(24);
    expect(prefixForHosts(2)).toBe(30);
    expect(prefixForHosts(0)).toBe(24); // fallback
    expect(prefixForHosts(NaN)).toBe(24);
  });

  it("masks octets to the network address", () => {
    expect(networkOctets([10, 44, 0, 37], 26)).toEqual([10, 44, 0, 0]);
    expect(networkOctets([192, 168, 1, 200], 26)).toEqual([192, 168, 1, 192]);
    expect(networkOctets([10, 44, 3, 5], 16)).toEqual([10, 44, 0, 0]);
  });

  it("builds a normalized network CIDR", () => {
    expect(buildCidr([10, 44, 0, 37], 26)).toBe("10.44.0.0/26");
    expect(buildCidr([192, 168, 1, 1], 24)).toBe("192.168.1.0/24");
  });

  it("parses valid CIDRs and rejects malformed ones", () => {
    expect(parseCidr("10.44.0.0/24")).toEqual({
      octets: [10, 44, 0, 0],
      prefix: 24,
    });
    expect(parseCidr(" 10.44.0.0/24 ")).not.toBeNull();
    expect(parseCidr("10.44.0.0")).toBeNull();
    expect(parseCidr("10.44.0.256/24")).toBeNull();
    expect(parseCidr("10.44.0.0/33")).toBeNull();
    expect(parseCidr("not a cidr")).toBeNull();
  });

  it("isValidCidr mirrors parseCidr", () => {
    expect(isValidCidr("10.0.0.0/8")).toBe(true);
    expect(isValidCidr("10.0.0.0")).toBe(false);
  });

  it("recognizes RFC 1918 private ranges", () => {
    expect(octetsPrivate([10, 0, 0, 0])).toBe(true);
    expect(octetsPrivate([172, 16, 0, 0])).toBe(true);
    expect(octetsPrivate([172, 31, 255, 0])).toBe(true);
    expect(octetsPrivate([172, 32, 0, 0])).toBe(false); // outside 172.16/12
    expect(octetsPrivate([172, 15, 0, 0])).toBe(false);
    expect(octetsPrivate([192, 168, 1, 0])).toBe(true);
    expect(octetsPrivate([192, 169, 0, 0])).toBe(false);
    expect(octetsPrivate([8, 8, 0, 0])).toBe(false); // public
    expect(octetsPrivate([100, 64, 0, 0])).toBe(false); // CGNAT is not RFC 1918
  });

  it("isPrivateCidr requires the whole range inside one RFC 1918 block", () => {
    expect(isPrivateCidr("10.44.0.0/24")).toBe(true);
    expect(isPrivateCidr("192.168.1.0/24")).toBe(true);
    expect(isPrivateCidr("172.16.0.0/12")).toBe(true); // the canonical /12
    expect(isPrivateCidr("10.0.0.0/15")).toBe(true); // within 10/8
    expect(isPrivateCidr("8.8.0.0/16")).toBe(false);
    // Straddlers: private base but the range extends into public space.
    expect(isPrivateCidr("192.168.0.0/15")).toBe(false); // → 192.169.x
    expect(isPrivateCidr("172.16.0.0/11")).toBe(false); // → 172.0-172.15 / 172.32+
    expect(isPrivateCidr("10.0.0.0/7")).toBe(false); // → 11.x
    expect(isPrivateCidr("not a cidr")).toBe(false);
  });
});
