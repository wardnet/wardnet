import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface FeatureCardProps {
  /** Icon element rendered at the top of the card. */
  icon: ReactNode;
  /** Feature title. */
  title: string;
  /** Short description of the feature. */
  description: string;
  className?: string;
}

/**
 * Displays a feature with an icon, title, and description inside a bordered card.
 */
export function FeatureCard({ icon, title, description, className }: FeatureCardProps) {
  return (
    <div
      className={cn(
        "rounded-xl border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-[oklch(0.18_0.02_270)]",
        className,
      )}
    >
      <div className="mb-4 text-[var(--brand-green)]">{icon}</div>
      <h3 className="mb-2 text-lg font-semibold text-gray-900 dark:text-gray-100">{title}</h3>
      <p className="text-sm leading-relaxed text-gray-500 dark:text-gray-400">{description}</p>
    </div>
  );
}
