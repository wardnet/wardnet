import type { ReactNode } from "react";
import { Link } from "react-router";
import { cn } from "@/lib/utils";

interface FeatureCardProps {
  /** Icon element rendered at the top of the card. */
  icon: ReactNode;
  /** Feature title. */
  title: string;
  /** Short description of the feature. */
  description: string;
  /**
   * Optional destination. When set, the whole card becomes a link to its
   * detail page. Internal paths (`/docs/...`) render as a react-router
   * `Link`; absolute URLs render as a plain anchor.
   */
  href?: string;
  /** When true, renders a small "Premium" tag in the card header. */
  premium?: boolean;
  className?: string;
}

/**
 * Displays a feature with an icon, title, and description inside a Forge
 * `.card`. The title acts as the headline stat and the description as the
 * sub line beneath it. When `href` is provided the entire card is clickable
 * and links to the feature's docs detail page.
 */
export function FeatureCard({
  icon,
  title,
  description,
  href,
  premium,
  className,
}: FeatureCardProps) {
  const content = (
    <>
      <div className="mb-4 flex items-center justify-between">
        <span className="text-accent">{icon}</span>
        {premium && (
          <span className="rounded-full bg-accent-soft px-2 py-0.5 t-size-xs t-weight-semibold uppercase tracking-wider text-accent">
            Premium
          </span>
        )}
      </div>
      <h3 className="mb-2 t-size-lg t-weight-semibold text-ink">{title}</h3>
      <p className="t-size-sm leading-relaxed text-ink-3">{description}</p>
    </>
  );

  if (href) {
    const cardClass = cn("card block transition-colors hover:border-line-strong", className);
    // Internal docs routes use the SPA router; absolute URLs escape to a
    // plain anchor so external links still work.
    const isExternal = /^https?:\/\//.test(href);
    return isExternal ? (
      <a href={href} className={cardClass}>
        {content}
      </a>
    ) : (
      <Link to={href} className={cardClass}>
        {content}
      </Link>
    );
  }

  return <div className={cn("card", className)}>{content}</div>;
}
