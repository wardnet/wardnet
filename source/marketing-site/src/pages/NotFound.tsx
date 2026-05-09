import { Compass, SearchX } from "lucide-react";
import { Link } from "react-router";
import { Logo } from "@/components/compound/Logo";

/**
 * Catch-all 404 view. Reached when React Router finds no matching route —
 * either because the user typed an unknown URL, followed a stale link, or
 * hit a path that hasn't been deployed yet.
 *
 * Visually mirrors the Hero section so the user stays grounded in the site's
 * look even when they're off the beaten path.
 */
export function NotFound() {
  return (
    <section className="relative flex min-h-screen flex-col items-center justify-center bg-gradient-to-br from-[oklch(0.22_0.12_275)] to-[oklch(0.16_0.08_260)] px-6 text-center">
      <Logo size={96} className="mb-8" />

      <div className="mb-3 inline-flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.3em] text-white/40">
        <SearchX size={14} aria-hidden="true" />
        Page not found
      </div>
      <h1 className="mb-3 text-4xl font-bold tracking-tight text-white sm:text-5xl">Off the map</h1>
      <p className="mb-10 max-w-md text-base leading-relaxed text-gray-400">
        This URL doesn't route anywhere. If you followed a link from somewhere, it's probably out of
        date.
      </p>

      <div className="flex w-full max-w-xs flex-col gap-4 sm:max-w-none sm:flex-row sm:justify-center">
        <Link
          to="/"
          className="inline-block w-full rounded-lg bg-[var(--brand-green)] px-8 py-3 text-center text-sm font-semibold text-white transition-colors hover:bg-[var(--brand-green-hover)] sm:w-48"
        >
          Back home
        </Link>
        <Link
          to="/docs"
          className="inline-flex w-full items-center justify-center gap-2 rounded-lg border border-white/20 bg-white/5 px-8 py-3 text-sm font-semibold text-white transition-colors hover:bg-white/10 sm:w-48"
        >
          <Compass size={16} aria-hidden="true" />
          Browse docs
        </Link>
      </div>
    </section>
  );
}
