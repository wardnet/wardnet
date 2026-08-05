import type { Device } from "@wardnet/js";

/**
 * Device search that tolerates the two things an admin actually types
 * (issue #1099): a MAC in whatever format their vendor app printed it, and a
 * MAC that belongs to the same physical gadget but is not the address it
 * associated with.
 */

/** How far from the searched address a {@link findNeighbourMacs} candidate may sit. */
const NEIGHBOUR_DISTANCE = 4;

/** A MAC is 6 bytes; 48 bits fits comfortably in a JS double (< 2^53). */
const MAC_HEX_LENGTH = 12;

/**
 * Reduce a MAC to bare lowercase hex, dropping every separator convention in
 * the wild: `5c:e7:53:4e:ec:db`, `5C-E7-53-4E-EC-DB`, `5ce7.534e.ecdb` and
 * `5CE7534EECDB` all collapse to `5ce7534eecdb`.
 *
 * Applied to both sides of a comparison so the stored format never has to match
 * whatever the admin pasted.
 */
export function normalizeMac(input: string): string {
  return input.replace(/[:.\-\s]/g, "").toLowerCase();
}

/** True when `value` is bare hex — i.e. plausibly a MAC or a MAC prefix. */
function isHexOnly(value: string): boolean {
  return value.length > 0 && /^[0-9a-f]+$/.test(value);
}

/**
 * True when the query is a dotted-decimal address or a fragment of one
 * (`10`, `10.0`, `192.168.1.5`) — something the caller means as an IP.
 *
 * MACs are written with `:` or `-`, never with a `.` between decimal octets, so
 * a dot-separated all-decimal string is unambiguously not a MAC search.
 */
function looksLikeIpv4(value: string): boolean {
  return value.includes(".") && /^[0-9.]+$/.test(value);
}

/**
 * Parse a full 12-hex-digit MAC into its numeric value, or `null` if the input
 * is not a complete MAC.
 *
 * Numeric because the neighbour search has to do arithmetic across byte
 * boundaries — see {@link findNeighbourMacs}.
 */
function macToNumber(mac: string): number | null {
  const hex = normalizeMac(mac);
  if (hex.length !== MAC_HEX_LENGTH || !isHexOnly(hex)) return null;
  return Number.parseInt(hex, 16);
}

/**
 * Shortest hex query allowed to match the *middle* of a MAC.
 *
 * A prefix match works at any length (searching a bare OUI is the point), but
 * an unanchored substring needs a floor or short hex fragments match half the
 * network — `ab` appears somewhere in a great many addresses. Four digits is
 * two octets, which is what someone reading the tail off a device label types.
 */
const MIN_MAC_SUBSTRING_LENGTH = 4;

/**
 * Whether a device matches a free-text query.
 *
 * Matches name, hostname, manufacturer, IP and MAC. The MAC comparison is
 * format-tolerant and prefix-capable, so searching a bare OUI (`5c:e7:53`)
 * lists every device from that vendor block rather than requiring the full
 * address.
 *
 * It also matches *within* the address, because the label on the back of a
 * device usually shows only the last few digits. Prefix-only matching would
 * have regressed that: the search this replaced did a plain `includes` on the
 * MAC, so `ecdb` used to find the device and would otherwise have started
 * returning nothing — with the "this may be the Bluetooth MAC" empty state
 * making the miss look meaningful.
 *
 * An empty query matches everything, so callers can pass the raw input box
 * value without guarding.
 */
export function matchesDevice(device: Device, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;

  const textHit = [
    device.name,
    device.hostname,
    device.manufacturer,
    device.last_ip,
  ].some((field) => (field ?? "").toLowerCase().includes(q));
  if (textHit) return true;

  // Compare MACs separator-insensitively. Only treat the query as a MAC when
  // it normalises to bare hex — otherwise a search for "e" would prefix-match
  // most of the network.
  //
  // Dotted-decimal is excluded explicitly: `normalizeMac` strips `.`, so the IP
  // "10.0.0.1" would otherwise collapse to "10001", pass the hex test, and
  // prefix-match every MAC starting `10:00:1…`. An IP search must not surface
  // unrelated devices.
  if (looksLikeIpv4(q)) return false;
  const needle = normalizeMac(q);
  if (!isHexOnly(needle)) return false;

  const mac = normalizeMac(device.mac);
  return (
    mac.startsWith(needle) ||
    (needle.length >= MIN_MAC_SUBSTRING_LENGTH && mac.includes(needle))
  );
}

/** A device offered as a near-miss for a searched MAC, with its offset. */
export interface NeighbourMatch {
  device: Device;
  /** Signed distance from the searched address; never 0 (that is an exact hit). */
  offset: number;
}

/**
 * Devices whose MAC sits within ±4 of the searched address.
 *
 * Answers the reported failure: vendor apps frequently print a device's
 * *Bluetooth* MAC while it associates over Wi-Fi under a different address.
 * Espressif — the chipset behind most of the affected smart-home gear —
 * derives all of its addresses from one base MAC, assigning Wi-Fi STA, Wi-Fi
 * AP, Bluetooth and Ethernet as base+0/+1/+2/+3. So the address in the admin's
 * hand is usually within a few of the one Wardnet saw.
 *
 * The arithmetic is over the full 48-bit value rather than the last octet: the
 * derived addresses cross the byte boundary (`…:ff` + 1 is `…:00` of the next
 * octet), and last-octet subtraction would score that pair 255 apart while
 * scoring two genuinely unrelated addresses as adjacent.
 *
 * Returns empty unless the query is a complete MAC — a partial address has no
 * meaningful neighbourhood. Results are sorted nearest-first, and an exact hit
 * is excluded because that is a match, not a guess.
 */
export function findNeighbourMacs(
  devices: readonly Device[],
  query: string,
): NeighbourMatch[] {
  const target = macToNumber(query.trim());
  if (target === null) return [];

  const matches: NeighbourMatch[] = [];
  for (const device of devices) {
    const value = macToNumber(device.mac);
    if (value === null) continue;
    const offset = value - target;
    if (offset !== 0 && Math.abs(offset) <= NEIGHBOUR_DISTANCE) {
      matches.push({ device, offset });
    }
  }

  return matches.sort((a, b) => Math.abs(a.offset) - Math.abs(b.offset));
}
