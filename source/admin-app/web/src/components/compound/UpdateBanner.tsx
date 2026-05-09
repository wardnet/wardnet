import { Link } from "react-router";

interface Props {
  /** Whether an update is available. When false, nothing is rendered. */
  updateAvailable: boolean;
  /** Latest version advertised by the release source. */
  latestVersion: string | null;
}

/**
 * Slim "update available" banner for sidebar/header placement.
 *
 * Pure presentation — visibility is decided by the caller based on
 * `useUpdateStatus`. Clicking the link navigates to Settings where the full
 * [`UpdateCard`](../features/UpdateCard.tsx) takes over.
 */
export function UpdateBanner({ updateAvailable, latestVersion }: Props) {
  if (!updateAvailable || !latestVersion) {
    return null;
  }
  return (
    <Link
      to="/settings"
      className="block rounded-md bg-primary/10 px-3 py-2 text-xs font-medium text-primary hover:bg-primary/15"
    >
      Update available: v{latestVersion}
    </Link>
  );
}
