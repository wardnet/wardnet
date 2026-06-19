import { Compass, SearchX } from "lucide-react";
import { Link } from "react-router";
import { Logo } from "@/components/compound/Logo";

/**
 * Catch-all 404 view. Reached when React Router finds no matching route —
 * either because the user typed an unknown URL, followed a stale link, or
 * hit a path that hasn't been deployed yet.
 *
 * Renders as a full-page Forge `.empty` block, matching the admin-app
 * NotFound port (slice T6-δ) so the 404 treatment is consistent across
 * site and admin.
 */
export function NotFound() {
  return (
    <section className="empty col min-h-screen items-center justify-center gap-4">
      <Logo height={72} variant="light" />

      <div className="row gap-8 t-size-xs t-weight-semibold uppercase tracking-[0.3em]">
        <SearchX size={14} aria-hidden="true" />
        Page not found
      </div>
      <h1 className="h-title t-size-4xl">Off the map</h1>
      <p className="h-sub max-w-md">
        This URL doesn't route anywhere. If you followed a link from somewhere, it's probably out of
        date.
      </p>

      <div className="row gap-12 wrap justify-center">
        <Link to="/" className="btn btn--primary">
          Back home
        </Link>
        <Link to="/docs" className="btn">
          <Compass size={16} aria-hidden="true" />
          Browse docs
        </Link>
      </div>
    </section>
  );
}
