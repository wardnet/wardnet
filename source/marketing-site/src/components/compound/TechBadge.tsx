import { cn } from "@/lib/utils";

interface TechBadgeProps {
  /** Text label displayed inside the badge. */
  label: string;
  className?: string;
}

/**
 * Pill-shaped badge for displaying a technology name. Uses Forge `.pill
 * .pill--ghost` (transparent surface, subtle border) with `.mono` so the
 * tech name reads as a code-style token rather than prose.
 */
export function TechBadge({ label, className }: TechBadgeProps) {
  return <span className={cn("pill pill--ghost mono", className)}>{label}</span>;
}
