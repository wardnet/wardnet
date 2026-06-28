/** IPv4 helpers shared across surfaces (UI form validation, etc.). */

/** True when `value` is a complete dotted-quad: four non-empty octets,
 *  each a number in 0–255. Partial input like "10.0.1." is rejected. */
export function isCompleteIpv4(value: string): boolean {
  const octets = value.split(".");
  if (octets.length !== 4) return false;
  return octets.every((o) => {
    if (o === "" || !/^\d{1,3}$/.test(o)) return false;
    const n = Number(o);
    return n >= 0 && n <= 255;
  });
}

/** Dotted-quad → 32-bit integer for range comparisons. Assumes a valid
 *  IPv4 (guard with {@link isCompleteIpv4} first). Multiply-and-add keeps
 *  the result positive where bitwise ops would coerce to signed 32-bit. */
export function ipv4ToInt(value: string): number {
  return value.split(".").reduce((acc, o) => acc * 256 + Number(o), 0);
}
