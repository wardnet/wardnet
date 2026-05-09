import { cn } from "@/lib/utils";

interface TechBadgeProps {
  /** Text label displayed inside the badge. */
  label: string;
  className?: string;
}

/**
 * Pill-shaped badge for displaying a technology name.
 */
export function TechBadge({ label, className }: TechBadgeProps) {
  return (
    <span
      className={cn(
        "inline-block rounded-full bg-gray-100 px-4 py-1.5 text-sm font-medium text-gray-700 dark:bg-[oklch(0.22_0.02_270)] dark:text-gray-300",
        className,
      )}
    >
      {label}
    </span>
  );
}
