import { releaseInfo } from "@/generated/release-info";

interface LatestReleaseBadgeProps {
  /** Visual variant — "dark" for the hero (light text on dark bg), "light" otherwise. */
  variant?: "dark" | "light";
  /** Optional className to merge into the root span. */
  className?: string;
}

/**
 * Small Forge `.pill` showing the latest stable release version, linking to
 * the GitHub Release notes. Renders nothing when no release has been
 * published yet (fresh repo, or the manifest generator couldn't reach the
 * API).
 *
 * The data comes from `src/generated/release-info.ts`, which is regenerated
 * on every site build by `scripts/generate-release-manifests.ts`.
 */
export function LatestReleaseBadge({ variant = "light", className }: LatestReleaseBadgeProps) {
  const release = releaseInfo.stable;
  if (!release || !release.version) {
    return null;
  }

  // The hero sits on Ward Navy chrome, where the default `.pill` surface
  // (`--bg-sunken`) reads as light grey; `.pill--ghost` gives us a
  // transparent surface with a subtle border that reads correctly on dark.
  const variantClass = variant === "dark" ? "pill pill--ghost" : "pill";

  return (
    <a
      href={release.notes_url}
      className={[variantClass, "transition-colors", className].filter(Boolean).join(" ")}
    >
      <span>Latest release</span>
      <span className="mono font-semibold">v{release.version}</span>
    </a>
  );
}
