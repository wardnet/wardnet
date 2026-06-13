/**
 * Wardnet Forge — design tokens
 *
 * Source of truth for the design language's semantic values. Every consumer
 * (forge-web, forge-native, charts that pick a series colour at runtime, etc.)
 * derives from this file. The web CSS in `forge/styles.css` is the CSS-var
 * manifestation of these same values for light/dark themes.
 *
 * The full token set is extracted incrementally as primitives or platforms
 * need it. Anything still living only in `styles.css` is a known gap, not
 * an intentional split.
 */

export const brand = {
  /** Primary brand accent — Wardnet Emerald. Single, locked across the system. */
  accent: "#12B981",
  /** Ink — text/marks on an emerald surface (e.g. primary buttons). */
  accentInk: "#11152B",
  /** Ink — sidebar / chrome on dark surfaces (formerly "Ward Navy"). */
  navy: "#11152B",
} as const;

export const status = {
  warn: "#f1b13b",
  danger: "#E5484D",
  info: "#4d8df6",
} as const;

export const radius = {
  sm: 6,
  md: 10,
  lg: 14,
  xl: 20,
} as const;

export const density = {
  /** Comfortable density — baked. No compact mode. */
  pad: 18,
  rowHeight: 52,
} as const;

export const font = {
  sans: '"Inter Tight", ui-sans-serif, system-ui, -apple-system, "Helvetica Neue", sans-serif',
  mono: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
} as const;
