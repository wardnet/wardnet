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
 *
 * Forge mapping (T3-δ): title uses `.h-title`, meta line uses `.h-sub`,
 * layout rows use `.row`/`.col` helpers. Breadcrumb mirrors the
 * `.topbar__crumbs` pattern (ink-3 trail, ink current segment) without
 * the topbar chrome, since this lives in the page body, not the chrome.
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
    <header className="col gap-8 pb-4">
      <nav aria-label="Breadcrumb" className="row gap-8 text-[13px] text-ink-3">
        <Link to={parentTo} className="text-ink-3 hover:text-ink">
          {parentLabel}
        </Link>
        <ChevronRight aria-hidden className="size-4 text-ink-4" />
        <span className="truncate font-semibold text-ink">{itemLabel}</span>
      </nav>
      <div className="row wrap gap-12">
        {icon ? (
          <span aria-hidden className="row text-ink-2">
            {icon}
          </span>
        ) : null}
        <h1 className="h-title">{itemLabel}</h1>
        {status}
      </div>
      {meta ? <p className="h-sub">{meta}</p> : null}
    </header>
  );
}
