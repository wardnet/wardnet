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
  /** Primary brand accent — Signal Green. Single, locked across the system. */
  accent: "#1ed68a",
  accentInk: "#042619",
  /** Ward Navy — sidebar / chrome on dark surfaces. */
  navy: "#0c1230",
} as const;

export const status = {
  warn: "#f1b13b",
  danger: "#e5484d",
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
