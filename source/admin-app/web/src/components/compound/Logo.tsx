interface LogoProps {
  size?: number;
  className?: string;
}

/**
 * Wardnet brand mark: a rounded shield in `--accent` (signal green) with a
 * signal arc + ping inked in the page surface. Two visual variants share one
 * SVG path set:
 *
 * - Chrome (size < 40px) — used in the Sidebar / AppLayout. The shield is
 *   filled solid; only the signal arcs and ping dot show through. Reads as a
 *   compact accent chip at 24-28px.
 * - Marketing (size >= 40px) — used in AuthLayout / launch screens. The
 *   shield gets a hairline `--ink` outline and the signal arcs gain a second,
 *   wider ring so the "signal" half of the mark is legible at 60px+.
 *
 * Colors come from Forge tokens (`--accent`, `--ink`, `--side-bg`) so the
 * mark retints automatically with the active theme.
 */
export function Logo({ size = 32, className }: LogoProps) {
  const isMarketing = size >= 40;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      role="img"
      aria-label="Wardnet"
      className={className}
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Shield silhouette — the "ward" half of the mark. */}
      <path
        d="M16 3.2 L26.4 6.4 V15.2 C26.4 21.6 21.76 26.72 16 28.8 C10.24 26.72 5.6 21.6 5.6 15.2 V6.4 Z"
        fill="var(--accent)"
        stroke={isMarketing ? "var(--ink)" : "none"}
        strokeWidth={isMarketing ? 0.8 : 0}
        strokeLinejoin="round"
      />
      {/* Outer signal arc — the "signal" half of the mark. */}
      {isMarketing && (
        <path
          d="M10.4 17.6 C10.4 14.5 12.96 12 16 12 C19.04 12 21.6 14.5 21.6 17.6"
          stroke="var(--side-bg)"
          strokeWidth="1.6"
          strokeLinecap="round"
          fill="none"
        />
      )}
      {/* Inner signal arc — present in both variants so chrome size still reads as "signal". */}
      <path
        d="M12.8 17.6 C12.8 15.84 14.24 14.4 16 14.4 C17.76 14.4 19.2 15.84 19.2 17.6"
        stroke="var(--side-bg)"
        strokeWidth="1.6"
        strokeLinecap="round"
        fill="none"
      />
      {/* Ping dot. */}
      <circle cx="16" cy="20" r="1.4" fill="var(--side-bg)" />
    </svg>
  );
}
