import { AlertTriangle } from "lucide-react";
import { useDaemonStatus } from "@/hooks/useDaemonStatus";

/**
 * Full-width red banner shown at the top of all pages when the daemon is unreachable.
 * Uses the same unauthenticated /api/info endpoint as the sidebar connection indicator.
 */
export function ConnectionBanner() {
  const { data } = useDaemonStatus();

  if (!data || data.reachable) return null;

  return (
    <div className="flex items-center gap-2 bg-red-100 px-4 py-2 text-sm text-red-800 dark:bg-red-950 dark:text-red-200">
      <AlertTriangle className="size-4 shrink-0" />
      <span>Unable to connect to the Wardnet daemon. Make sure it is running.</span>
    </div>
  );
}
