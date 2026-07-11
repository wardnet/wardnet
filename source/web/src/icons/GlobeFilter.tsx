import type { SVGProps } from "react";

/**
 * Globe with a filter badge in the top-right — the "DNS filtering" mark.
 * Built like lucide's `globe-lock` (an open globe trimmed on the right to
 * seat a badge), swapping the lock for lucide's `list-filter` bars rotated
 * 180° (widest bar at the bottom) so it reads as a filter.
 *
 * lucide-compatible: renders a 24×24 `currentColor` stroked SVG and spreads
 * any props (the sidebar passes `className="ico"`, which sizes it via CSS).
 */
export function GlobeFilter(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={24}
      height={24}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      {/* Open globe (from lucide `globe-lock`): a near-full globe whose
          right side is trimmed to seat the badge, plus a left-half equator. */}
      <path d="M15.686 15A14.5 14.5 0 0 1 12 22a14.5 14.5 0 0 1 0-20 10 10 0 1 0 9.542 13" />
      <path d="M2 12h8.5" />
      {/* Filter badge, seated in the globe's top-right cutout (like the lock
          in `globe-lock`): lucide `list-filter`'s three decreasing bars,
          rotated 180° so the widest bar is at the bottom. */}
      <g transform="translate(12.32 1.8) scale(0.44)" strokeWidth={3.9}>
        <path d="M9 5h6" />
        <path d="M6 12h12" />
        <path d="M2 19h20" />
      </g>
    </svg>
  );
}
