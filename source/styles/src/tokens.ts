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

/**
 * Type scale — the dense Forge ramp that overrides Tailwind's default
 * `text-*` sizes (see docs/adr-typography-scale-and-roles.md). `size` is the
 * `rem` value at a 16px root; `lineHeight` is the unitless paired leading.
 * Mirrored as CSS vars in `styles.css` (`--text-*` / `--text-*--line-height`)
 * and as Tailwind theme keys in `theme.css`; all three must stay in sync.
 */
export const text = {
  "2xs": { size: "0.6875rem", lineHeight: 1.3 }, //  11px — absorbs 10, 11, 11.5
  xs: { size: "0.75rem", lineHeight: 1.35 }, //     12px — Tailwind default
  sm: { size: "0.8125rem", lineHeight: 1.5 }, //    13px — body (was 14)
  base: { size: "0.875rem", lineHeight: 1.5 }, //   14px — (was 16)
  lg: { size: "1rem", lineHeight: 1.4 }, //         16px — absorbs 15, 17
  xl: { size: "1.125rem", lineHeight: 1.3 }, //     18px
  "2xl": { size: "1.375rem", lineHeight: 1.2 }, //  22px — (was 24)
  "3xl": { size: "1.625rem", lineHeight: 1.15 }, // 26px — (was 30)
  "4xl": { size: "2rem", lineHeight: 1.05 }, //     32px — metric (was 36)
} as const;
