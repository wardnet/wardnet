import { Link } from "react-router";
import { ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

interface DetailPageHeaderProps {
  /** The resource list label (e.g. "Tunnels"). */
  parentLabel: string;
  /** The route to the parent list page (e.g. "/tunnels"). */
  parentTo: string;
  /** The current item's label — usually the title row. */
  itemLabel: string;
  /** Optional icon rendered inline left of the title. */
  icon?: ReactNode;
  /** Optional content rendered to the right of the title (status pill, etc.). */
  status?: ReactNode;
  /** Optional content rendered below the title row (last-handshake age, etc.). */
  meta?: ReactNode;
}

/**
 * Standard chrome for routed detail pages: breadcrumb + title + optional
 * status pill + optional trailing meta line.
 *
 * Established by the tunnel detail page; other detail pages should adopt
 * this header as part of the routed-detail refactor (issue #316).
 */
export function DetailPageHeader({
  parentLabel,
  parentTo,
  itemLabel,
  icon,
  status,
  meta,
}: DetailPageHeaderProps) {
  return (
    <header className="flex flex-col gap-2 pb-4">
      <nav
        aria-label="Breadcrumb"
        className="flex items-center gap-1 text-sm text-muted-foreground"
      >
        <Link to={parentTo} className="hover:text-ink">
          {parentLabel}
        </Link>
        <ChevronRight aria-hidden className="size-4" />
        <span className="truncate">{itemLabel}</span>
      </nav>
      <div className="flex flex-wrap items-center gap-3">
        {icon ? (
          <span aria-hidden className="inline-flex items-center text-ink/70">
            {icon}
          </span>
        ) : null}
        <h1 className="text-2xl font-semibold tracking-tight">{itemLabel}</h1>
        {status}
      </div>
      {meta ? <div className="text-sm text-muted-foreground">{meta}</div> : null}
    </header>
  );
}
