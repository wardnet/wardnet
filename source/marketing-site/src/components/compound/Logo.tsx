import logoDark from "@/assets/wardnet-logo-dark.svg";
import logoLight from "@/assets/wardnet-logo-light.svg";
import logoTaglineDark from "@/assets/wardnet-logo-tagline-dark.svg";
import logoTaglineLight from "@/assets/wardnet-logo-tagline-light.svg";

interface LogoProps {
  /** Rendered height in px; width scales with the lockup's aspect ratio. */
  height?: number;
  /**
   * Background context: "dark" → white "WARD" (for dark surfaces),
   * "light" → ink "WARD" (for light surfaces). "NET" is always emerald.
   */
  variant?: "light" | "dark";
  /** Use the lockup that includes the tagline (hero / primary brand moment). */
  tagline?: boolean;
  className?: string;
}

/** Wardnet logo lockup (mark + WARDNET wordmark) from the brand assets. */
export function Logo({ height = 24, variant = "dark", tagline = false, className }: LogoProps) {
  const src = tagline
    ? variant === "dark"
      ? logoTaglineDark
      : logoTaglineLight
    : variant === "dark"
      ? logoDark
      : logoLight;
  return <img src={src} alt="Wardnet" style={{ height, width: "auto" }} className={className} />;
}
