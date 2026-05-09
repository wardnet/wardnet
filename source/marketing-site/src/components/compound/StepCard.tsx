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
 * Numbered step card used in the "How it works" section.
 */
export function StepCard({ step, title, description, className }: StepCardProps) {
  return (
    <div className={cn("text-center", className)}>
      <div className="mb-3 text-5xl font-bold text-[var(--brand-green)]">{step}</div>
      <h3 className="mb-2 text-lg font-semibold text-gray-900 dark:text-gray-100">{title}</h3>
      <p className="text-sm leading-relaxed text-gray-500 dark:text-gray-400">{description}</p>
    </div>
  );
}
