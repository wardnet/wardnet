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
 * Displays a feature with an icon, title, and description inside a Forge
 * `.card`. The title acts as the headline stat and the description as the
 * sub line beneath it.
 */
export function FeatureCard({ icon, title, description, className }: FeatureCardProps) {
  return (
    <div className={cn("card", className)}>
      <div className="mb-4 text-accent">{icon}</div>
      <h3 className="mb-2 text-lg font-semibold text-ink">{title}</h3>
      <p className="text-sm leading-relaxed text-ink-3">{description}</p>
    </div>
  );
}
