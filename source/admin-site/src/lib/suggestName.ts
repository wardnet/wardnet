/**
 * Two-word hostname suggestions for the remote-access wizard step.
 *
 * Produces `adjective-scientist` slugs (e.g. `happy-einstein`) the operator can
 * accept or override. The generator deliberately mirrors the bridge's own name
 * rules (see `source/bridge/src/api/validation.rs`) so a suggestion never
 * fails registration: 3–32 chars, lowercase `[a-z0-9-]`, no leading/trailing
 * hyphen, and not a reserved label. Words are kept short so the joined slug
 * stays well under 32 characters.
 */

const ADJECTIVES = [
  "happy",
  "brave",
  "calm",
  "clever",
  "gentle",
  "swift",
  "bright",
  "quiet",
  "cosmic",
  "lunar",
  "solar",
  "amber",
  "azure",
  "coral",
  "jade",
  "noble",
  "merry",
  "witty",
  "zesty",
  "bold",
] as const;

const SCIENTISTS = [
  "einstein",
  "curie",
  "newton",
  "tesla",
  "darwin",
  "bohr",
  "hawking",
  "lovelace",
  "turing",
  "kepler",
  "galileo",
  "faraday",
  "pasteur",
  "noether",
  "feynman",
  "planck",
  "hopper",
  "franklin",
  "mendel",
  "fermi",
] as const;

/** Bridge name validation, mirrored client-side for instant feedback. */
const RESERVED = new Set([
  "www",
  "mail",
  "api",
  "ddns",
  "my",
  "admin",
  "bridge",
  "static",
  "wildcard",
  "wardnet",
  "support",
  "help",
  "ns",
  "ns1",
  "ns2",
  "ftp",
  "smtp",
  "imap",
  "pop3",
  "us",
  "eu",
]);

/** Whether `name` satisfies the bridge's naming constraints. */
export function isValidName(name: string): boolean {
  if (name.length < 3 || name.length > 32) return false;
  if (name.startsWith("-") || name.endsWith("-")) return false;
  if (!/^[a-z0-9-]+$/.test(name)) return false;
  return !RESERVED.has(name);
}

/** Whether `name` is well-formed but reserved (so the UI can say so precisely). */
export function isReservedName(name: string): boolean {
  return RESERVED.has(name);
}

function pick<T>(items: readonly T[]): T {
  return items[Math.floor(Math.random() * items.length)];
}

/** A fresh, valid `adjective-scientist` suggestion. */
export function suggestName(): string {
  return `${pick(ADJECTIVES)}-${pick(SCIENTISTS)}`;
}
