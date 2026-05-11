import logoSrc from "@/assets/logo.png";

interface LogoProps {
  size?: number;
  className?: string;
}

/** Wardnet brand mark — sourced from `src/assets/logo.png` (192px master). */
export function Logo({ size = 32, className }: LogoProps) {
  return <img src={logoSrc} alt="Wardnet" width={size} height={size} className={className} />;
}
