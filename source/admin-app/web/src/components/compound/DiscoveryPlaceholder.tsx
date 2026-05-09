import { useEffect, useMemo, useState } from "react";
import { Radar } from "lucide-react";

interface DiscoveryPlaceholderProps {
  /** Number of skeleton columns to show. */
  cols?: number;
  /** Primary status text. */
  message?: string;
  /** Secondary hint text. */
  hint?: string;
}

/**
 * Animated empty-state placeholder for tables waiting on dynamic data.
 *
 * Fills the remaining viewport height with a skeletal table structure,
 * scanning bar, floating particles, and a pulsing radar icon.
 */
export function DiscoveryPlaceholder({
  cols = 5,
  message = "Searching for network devices",
  hint = "Devices will appear as they are detected on the network.",
}: DiscoveryPlaceholderProps) {
  const [dots, setDots] = useState("");
  useEffect(() => {
    const interval = setInterval(() => {
      setDots((d) => (d.length >= 3 ? "" : d + "."));
    }, 500);
    return () => clearInterval(interval);
  }, []);

  // Generate many rows to fill the space — CSS will clip.
  const rows = 12;

  // Spread particles across the full grid.
  const particles = useMemo(
    () =>
      Array.from({ length: 14 }, (_, i) => ({
        top: `${8 + ((i * 7.3) % 75)}%`,
        left: `${5 + ((i * 17.1 + 11) % 90)}%`,
        delay: i * 0.25,
        size: i % 3 === 0 ? "size-2" : "size-1.5",
      })),
    [],
  );

  return (
    <div className="relative flex min-h-64 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-background">
      <style>{`
        @keyframes scan-sweep {
          0% { top: -3rem; opacity: 0; }
          5% { opacity: 1; }
          95% { opacity: 1; }
          100% { top: calc(100% + 3rem); opacity: 0; }
        }
        @keyframes particle-glow {
          0%, 100% { opacity: 0; transform: scale(0); }
          25% { opacity: 0.7; transform: scale(1.8); }
          50% { opacity: 0.3; transform: scale(1); }
        }
        @keyframes row-breathe {
          0%, 100% { opacity: 0.04; }
          50% { opacity: 0.1; }
        }
      `}</style>

      {/* Skeleton grid — fills the flex area, clips overflow */}
      <div className="relative min-h-0 flex-1 overflow-hidden px-4 py-3">
        {/* Header row */}
        <div className="flex gap-4 border-b border-border/30 pb-3">
          {Array.from({ length: cols }).map((_, i) => (
            <div
              key={`h-${i}`}
              className="h-3 flex-1 rounded bg-muted-foreground/10"
              style={{ animation: `row-breathe 3s ease-in-out ${i * 0.2}s infinite` }}
            />
          ))}
        </div>

        {/* Data rows — enough to overflow, parent clips */}
        {Array.from({ length: rows }).map((_, row) => (
          <div key={`r-${row}`} className="flex gap-4 border-b border-border/15 py-4">
            {Array.from({ length: cols }).map((_, col) => (
              <div
                key={`c-${row}-${col}`}
                className="h-2.5 flex-1 rounded bg-muted-foreground/[0.04]"
                style={{
                  animation: `row-breathe 3s ease-in-out ${(row + col) * 0.15}s infinite`,
                  maxWidth: col === 0 ? "35%" : col === cols - 1 ? "12%" : undefined,
                }}
              />
            ))}
          </div>
        ))}

        {/* Full-height scanning bar */}
        <div
          className="pointer-events-none absolute inset-x-0 h-12 bg-gradient-to-b from-transparent via-primary/[0.08] to-transparent"
          style={{ animation: "scan-sweep 4s ease-in-out infinite" }}
        />

        {/* Particles spread across the full area */}
        {particles.map((p, i) => (
          <div
            key={i}
            className={`absolute rounded-full bg-primary/40 ${p.size}`}
            style={{
              top: p.top,
              left: p.left,
              animation: `particle-glow 2.5s ease-in-out ${p.delay}s infinite`,
            }}
          />
        ))}
      </div>

      {/* Status area — always visible at the bottom */}
      <div className="flex shrink-0 flex-col items-center gap-3 border-t border-border/20 px-4 py-6">
        <div className="relative flex items-center justify-center">
          <Radar className="size-12 text-primary/25 animate-pulse" />
          <div className="absolute size-12 animate-ping rounded-full bg-primary/5" />
        </div>
        <p className="text-sm font-medium text-muted-foreground">
          {message}
          <span className="inline-block w-4">{dots}</span>
        </p>
        <p className="text-xs text-muted-foreground/60">{hint}</p>
      </div>
    </div>
  );
}
