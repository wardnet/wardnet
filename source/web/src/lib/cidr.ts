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

/**
 * True when the octets fall in an RFC 1918 private range:
 * `10.0.0.0/8`, `172.16.0.0/12`, or `192.168.0.0/16`. LAN subnets and DHCP
 * pools must be private — a public range would blackhole real internet hosts.
 */
export function octetsPrivate(
  octets: [number, number, number, number],
): boolean {
  const [a, b] = octets;
  if (a === 10) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 192 && b === 168) return true;
  return false;
}

/** The three RFC 1918 private blocks, as `{ octets, prefix }`. */
const RFC1918_BLOCKS: {
  octets: [number, number, number, number];
  prefix: number;
}[] = [
  { octets: [10, 0, 0, 0], prefix: 8 },
  { octets: [172, 16, 0, 0], prefix: 12 },
  { octets: [192, 168, 0, 0], prefix: 16 },
];

/**
 * True when the *entire* `cidr` range sits inside one RFC 1918 block. Checking
 * only the base address is not enough: a supernet like `192.168.0.0/15` has a
 * private base but straddles into public `192.169.x`. A subnet is fully private
 * iff its prefix is at least the block's prefix and its base masks to the block.
 */
export function isPrivateCidr(cidr: string): boolean {
  const parsed = parseCidr(cidr);
  if (!parsed) return false;
  return RFC1918_BLOCKS.some((block) => {
    if (parsed.prefix < block.prefix) return false;
    const net = networkOctets(parsed.octets, block.prefix);
    // eslint-disable-next-line security/detect-object-injection -- i is the 0-3 index from .every() over a fixed 4-octet tuple
    return net.every((o, i) => o === block.octets[i]);
  });
}

/** True when a dotted IPv4 address parses and is RFC 1918 private. */
export function isPrivateIpv4(ip: string): boolean {
  const parts = ip.split(".");
  if (parts.length !== 4) return false;
  const nums = parts.map((p) => (p === "" ? NaN : Number(p)));
  return (
    octetsValid(nums) && octetsPrivate(nums as [number, number, number, number])
  );
}
