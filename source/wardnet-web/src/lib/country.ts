/** Convert an ISO 3166-1 alpha-2 country code to its emoji flag. */
export function countryFlag(code: string): string {
  const upper = code.toUpperCase();
  if (upper.length !== 2) return "";
  return String.fromCodePoint(...upper.split("").map((c) => 0x1f1e6 + c.charCodeAt(0) - 65));
}
