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
 * Visually matches the NotFound page so users see a consistent "something's
 * off" styling rather than a raw browser error.
 */
export function ErrorView({ message, onRetry }: ErrorViewProps) {
  return (
    <section className="relative flex min-h-screen flex-col items-center justify-center bg-gradient-to-br from-[oklch(0.22_0.12_275)] to-[oklch(0.16_0.08_260)] px-6 text-center">
      <Logo size={96} className="mb-8" />

      <div className="mb-3 inline-flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.3em] text-white/40">
        <AlertTriangle size={14} aria-hidden="true" />
        Unexpected error
      </div>
      <h1 className="mb-3 text-4xl font-bold tracking-tight text-white sm:text-5xl">
        Something broke on our end
      </h1>
      <p className="mb-2 max-w-md text-base leading-relaxed text-gray-400">
        The page hit an error while rendering. Try reloading — if it keeps happening, file an issue
        and include what you were doing.
      </p>
      {message && (
        <p className="mb-10 max-w-md rounded-md border border-white/10 bg-white/5 px-4 py-2 font-mono text-xs text-gray-300">
          {message}
        </p>
      )}
      {!message && <div className="mb-10" />}

      <div className="flex w-full max-w-xs flex-col gap-4 sm:max-w-none sm:flex-row sm:justify-center">
        <button
          type="button"
          onClick={onRetry ?? (() => window.location.reload())}
          className="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-[var(--brand-green)] px-8 py-3 text-sm font-semibold text-white transition-colors hover:bg-[var(--brand-green-hover)] sm:w-48"
        >
          <RefreshCw size={16} aria-hidden="true" />
          Try again
        </button>
        <a
          href="https://github.com/wardnet/wardnet/issues/new"
          className="inline-flex w-full items-center justify-center rounded-lg border border-white/20 bg-white/5 px-8 py-3 text-sm font-semibold text-white transition-colors hover:bg-white/10 sm:w-48"
        >
          Report issue
        </a>
      </div>
    </section>
  );
}
