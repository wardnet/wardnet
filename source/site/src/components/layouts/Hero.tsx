import { ChevronDown } from "lucide-react";
import { LatestReleaseBadge } from "@/components/compound/LatestReleaseBadge";
import { Logo } from "@/components/compound/Logo";

interface HeroProps {
  /** Callback fired when the user clicks the Explore button. */
  onExplore: () => void;
}

/**
 * Full-viewport hero section with the Wardnet logo, tagline, and call-to-action buttons.
 */
export function Hero({ onExplore }: HeroProps) {
  return (
    <section className="relative flex min-h-screen flex-col items-center justify-center bg-gradient-to-br from-[oklch(0.22_0.12_275)] to-[oklch(0.16_0.08_260)] px-6 text-center">
      <Logo size={128} className="mb-8" />
      <h1 className="mb-3 text-5xl font-bold tracking-tight text-white sm:text-6xl">Wardnet</h1>
      <p className="mb-2 text-xl text-gray-300 sm:text-2xl">Your network. Your rules.</p>
      <p className="mb-6 max-w-xl text-base leading-relaxed text-gray-400">
        A self-hosted privacy gateway for Raspberry Pi. Per-device VPN routing, DNS ad blocking, and
        a web dashboard — all in a single binary.
      </p>
      <LatestReleaseBadge variant="dark" className="mb-8" />
      <div className="flex w-full max-w-xs flex-col gap-4 sm:max-w-none sm:flex-row sm:justify-center">
        <a
          href="https://github.com/wardnet/wardnet/releases"
          className="inline-block w-full rounded-lg bg-[var(--brand-green)] px-8 py-3 text-center text-sm font-semibold text-white transition-colors hover:bg-[var(--brand-green-hover)] sm:w-48"
        >
          Download
        </a>
        <a
          href="https://github.com/wardnet/wardnet"
          className="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-[#24292f] px-8 py-3 text-sm font-semibold text-white transition-colors hover:bg-[#32383f] sm:w-48"
        >
          <svg viewBox="0 0 16 16" className="h-4 w-4 fill-current" aria-hidden="true">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
          </svg>
          View on GitHub
        </a>
      </div>
      <button
        onClick={onExplore}
        className="absolute bottom-8 flex flex-col items-center gap-2 text-white/60 transition-colors hover:text-white/90"
        aria-label="Scroll to features"
      >
        <span className="text-sm font-bold uppercase tracking-widest">Explore</span>
        <ChevronDown size={32} strokeWidth={2.5} className="animate-bounce" />
      </button>
    </section>
  );
}
