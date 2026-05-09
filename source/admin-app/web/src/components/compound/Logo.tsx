import logoSrc from "@/assets/logo.png";

interface LogoProps {
  size?: number;
  className?: string;
}

/** Wardnet shield logo. */
export function Logo({ size = 32, className }: LogoProps) {
  return (
    <img
      src={logoSrc}
      alt="Wardnet"
      width={size}
      height={size}
      className={`rounded-md ${className ?? ""}`}
    />
  );
}
