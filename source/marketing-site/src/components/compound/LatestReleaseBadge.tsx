import { releaseInfo } from "@/generated/release-info";

interface LatestReleaseBadgeProps {
  /** Visual variant — "dark" for the hero (on Ward Navy), "light"
   *  for sections sitting on the cream / sunken page surface. */
  variant?: "dark" | "light";
  /** Optional className to merge into the root span. */
  className?: string;
}

/**
 * Accent-green pill linking to the GitHub Release notes for the
 * latest stable build. Uses `.pill--ok` (accent-soft surface +
 * accent-soft ink) so it stays visibly interactive against both the
 * Ward Navy hero and the cream-sunken GetStarted section — the
 * default `.pill` washes out against `bg-sunken`.
 *
 * Hover gets an underline + slight surface deepen so the element
 * unambiguously reads as a link rather than a static tag.
 *
 * The data comes from `src/generated/release-info.ts`, which is
 * regenerated on every site build by
 * `scripts/generate-release-manifests.ts`.
 */
export function LatestReleaseBadge({ className }: LatestReleaseBadgeProps) {
  const release = releaseInfo.stable;
  if (!release || !release.version) {
    return null;
  }

  return (
    <a
      href={release.notes_url}
      className={[
        "pill pill--ok",
        "transition-colors hover:underline hover:underline-offset-2",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <span>Latest release</span>
      <span className="mono t-weight-semibold">v{release.version}</span>
    </a>
  );
}
