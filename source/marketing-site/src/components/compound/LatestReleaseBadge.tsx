import { releaseInfo } from "@/generated/release-info";

interface LatestReleaseBadgeProps {
  /** Visual variant — "dark" for the hero (light text on dark bg), "light" otherwise. */
  variant?: "dark" | "light";
  /** Optional className to merge into the root span. */
  className?: string;
}

/**
 * Small pill showing the latest stable release version, linking to the
 * GitHub Release notes. Renders nothing when no release has been published
 * yet (fresh repo, or the manifest generator couldn't reach the API).
 *
 * The data comes from `src/generated/release-info.ts`, which is regenerated
 * on every site build by `scripts/generate-release-manifests.ts`.
 */
export function LatestReleaseBadge({ variant = "light", className }: LatestReleaseBadgeProps) {
  const release = releaseInfo.stable;
  if (!release || !release.version) {
    return null;
  }

  const baseClasses =
    "inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium transition-colors";
  const variantClasses =
    variant === "dark"
      ? "bg-white/10 text-white/80 hover:bg-white/15 hover:text-white"
      : "bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700";

  return (
    <a
      href={release.notes_url}
      className={[baseClasses, variantClasses, className].filter(Boolean).join(" ")}
    >
      <span className={variant === "dark" ? "text-white/50" : "text-gray-500 dark:text-gray-500"}>
        Latest release
      </span>
      <span className="font-semibold">v{release.version}</span>
    </a>
  );
}
