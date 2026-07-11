import type { SVGProps } from "react";

/**
 * Shield with a WiFi fan inside — the "VPN / remote access into the home
 * network" mark. Composed rather than from lucide because no single lucide
 * glyph reads as "secure inbound VPN" without colliding with the Tunnels
 * (outbound) icon.
 *
 * lucide-compatible: renders a 24×24 `currentColor` stroked SVG and spreads
 * any props (the sidebar passes `className="ico"`, which sizes it via CSS).
 */
export function ShieldWifi(props: SVGProps<SVGSVGElement>) {
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
      <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />
      <g transform="translate(6.48 5.06) scale(0.46)" strokeWidth={3.3}>
        <path d="M12 20h.01" />
        <path d="M2 8.82a15 15 0 0 1 20 0" />
        <path d="M5 12.859a10 10 0 0 1 14 0" />
        <path d="M8.5 16.429a5 5 0 0 1 7 0" />
      </g>
    </svg>
  );
}
