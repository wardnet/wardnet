/**
 * IPv4 CIDR helpers shared across apps (e.g. the zone-subnet input).
 *
 * All functions operate on IPv4 only. A "prefix" is the mask length (0–32).
 * "Usable hosts" excludes the network and broadcast addresses for prefixes
 * ≤ 30; /31 and /32 have no usable-host concept (point-to-point / single host)
 * and report 0.
 */

/** Parsed CIDR: four octets (0–255) plus a prefix length (0–32). */
export interface ParsedCidr {
  octets: [number, number, number, number];
  prefix: number;
}

/** True when every octet is an integer in 0–255. */
export function octetsValid(octets: number[]): boolean {
  return (
    octets.length === 4 &&
    octets.every((o) => Number.isInteger(o) && o >= 0 && o <= 255)
  );
}

/** Number of usable host addresses for a prefix (0 for /31 and /32). */
export function usableHosts(prefix: number): number {
  if (prefix < 0 || prefix > 32) return 0;
  if (prefix >= 31) return 0;
  return 2 ** (32 - prefix) - 2;
}

/**
 * Smallest network (largest prefix) whose usable-host count covers `hosts`.
 * Clamps to a sane range: at least a /30 (2 hosts), at most a /8. Returns 24
 * for non-positive / non-finite input.
 */
export function prefixForHosts(hosts: number): number {
  if (!Number.isFinite(hosts) || hosts <= 0) return 24;
  for (let prefix = 30; prefix >= 8; prefix--) {
    if (usableHosts(prefix) >= hosts) return prefix;
  }
  return 8;
}

/** Apply a prefix mask to octets, returning the network address octets. */
export function networkOctets(
  octets: [number, number, number, number],
  prefix: number,
): [number, number, number, number] {
  const value =
    ((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]) >>>
    0;
  // `>>> 0` keeps the result an unsigned 32-bit int after masking.
  const mask = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  const net = (value & mask) >>> 0;
  return [
    (net >>> 24) & 0xff,
    (net >>> 16) & 0xff,
    (net >>> 8) & 0xff,
    net & 0xff,
  ];
}

/** Build a normalized network CIDR string from octets + prefix. */
export function buildCidr(
  octets: [number, number, number, number],
  prefix: number,
): string {
  const net = networkOctets(octets, prefix);
  return `${net.join(".")}/${prefix}`;
}

/** Parse a CIDR string (e.g. "10.44.0.0/24"); returns null if malformed. */
export function parseCidr(cidr: string): ParsedCidr | null {
  const match = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\/(\d{1,2})$/.exec(
    cidr.trim(),
  );
  if (!match) return null;
  const octets = [
    Number(match[1]),
    Number(match[2]),
    Number(match[3]),
    Number(match[4]),
  ] as [number, number, number, number];
  const prefix = Number(match[5]);
  if (!octetsValid(octets) || prefix < 0 || prefix > 32) return null;
  return { octets, prefix };
}

/** True when `cidr` is a syntactically valid IPv4 CIDR. */
export function isValidCidr(cidr: string): boolean {
  return parseCidr(cidr) !== null;
}
