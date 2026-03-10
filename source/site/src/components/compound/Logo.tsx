import { cn } from "@/lib/utils";
import logoSrc from "@/assets/logo.png";

interface LogoProps {
  /** Logo dimensions in pixels. Defaults to 48. */
  size?: number;
  className?: string;
}

/**
 * Renders the Wardnet logo image at a configurable size.
 */
export function Logo({ size = 48, className }: LogoProps) {
  return (
    <img
      src={logoSrc}
      alt="Wardnet logo"
      width={size}
      height={size}
      className={cn("inline-block", className)}
    />
  );
}
