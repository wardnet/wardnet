import { AlertTriangle, RefreshCw } from "lucide-react";
import { Logo } from "@/components/compound/Logo";

interface ErrorViewProps {
  /** The error message to surface. Kept short — full details go to the
   *  console/reporter, not the UI. */
  message?: string;
  /** Called when the user clicks "Try again". Typically re-runs the query
   *  that threw, or forces a reload. */
  onRetry?: () => void;
}

/**
 * Generic error screen rendered by [`ErrorBoundary`] when an uncaught
 * exception bubbles up, and by Suspense/data-loader failures that choose
 * to surface through the boundary.
 *
 * Renders as a full-page Forge `.empty` block, mirroring the admin-app
 * NotFound port (slice T6-δ) so site and admin share the same "off the
 * happy path" treatment.
 */
export function ErrorView({ message, onRetry }: ErrorViewProps) {
  return (
    <section className="empty col min-h-screen items-center justify-center gap-4">
      <Logo height={72} variant="light" />

      <div className="row gap-8 t-size-xs t-weight-semibold uppercase tracking-[0.3em]">
        <AlertTriangle size={14} aria-hidden="true" />
        Unexpected error
      </div>
      <h1 className="h-title">Something broke on our end</h1>
      <p className="h-sub max-w-md">
        The page hit an error while rendering. Try reloading — if it keeps happening, file an issue
        and include what you were doing.
      </p>
      {message && <p className="kbd max-w-md">{message}</p>}

      <div className="row gap-12 wrap justify-center">
        <button
          type="button"
          onClick={onRetry ?? (() => window.location.reload())}
          className="btn btn--primary"
        >
          <RefreshCw size={16} aria-hidden="true" />
          Try again
        </button>
        <a href="https://github.com/wardnet/wardnet/issues/new" className="btn">
          Report issue
        </a>
      </div>
    </section>
  );
}
