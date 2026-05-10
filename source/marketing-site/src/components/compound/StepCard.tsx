import { cn } from "@/lib/utils";

interface StepCardProps {
  /** Step number displayed prominently. */
  step: number;
  /** Step title. */
  title: string;
  /** Step description. */
  description: string;
  className?: string;
}

/**
 * Numbered step card used in the "How it works" section. Renders inside a
 * Forge `.card` with the step number as display type via `.h-title` and the
 * accent token for emphasis.
 */
export function StepCard({ step, title, description, className }: StepCardProps) {
  return (
    <div className={cn("card text-center", className)}>
      <div className="h-title mb-3 text-accent">{step}</div>
      <h3 className="mb-2 text-lg font-semibold text-ink">{title}</h3>
      <p className="text-sm leading-relaxed text-ink-3">{description}</p>
    </div>
  );
}
