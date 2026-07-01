import { useRef } from "react";
import { RefreshCw } from "lucide-react";
import { Button } from "@wardnet/ui";
import { useDaemonStatus } from "../hooks/useDaemonStatus";

/**
 * Blocks the app behind a full-screen "can't reach the daemon" page until the
 * very first successful `/api/info` probe.
 *
 * The gate only blocks on a *cold* start (we've never reached the daemon). Once
 * a healthy connection has been seen, transient drops are handled by the
 * non-blocking `ConnectionBanner` instead — a live session (e.g. mid-restart,
 * which has its own dialog flow) must not be yanked to a full-screen error.
 *
 * Mount this around the app root *and* any always-on stream managers so they
 * don't open sockets (and reconnect-storm) while the daemon is unreachable.
 * Recovery is automatic: `useDaemonStatus` polls every 5 s while down, so the
 * gate lifts within a few seconds of the daemon coming back.
 */
export function ConnectionGate({ children }: { children: React.ReactNode }) {
  const { data, isLoading, isFetching, refetch } = useDaemonStatus();
  const hasConnected = useRef(false);

  // Latch ref: once we've seen a reachable daemon, stay unblocked forever.
  // Writing and reading the ref in render is deliberate (the latch flips on
  // the same render the query resolves), so opt these two lines out of the
  // render-time ref rule.
  // eslint-disable-next-line react-hooks/refs
  if (data?.reachable) hasConnected.current = true;

  // Reached the daemon at least once — never block again.
  // eslint-disable-next-line react-hooks/refs
  if (hasConnected.current) return <>{children}</>;

  // First probe still in flight — show a quiet splash, not a flash of error.
  if (isLoading || !data) return <ConnectionSplash />;

  if (!data.reachable) {
    return (
      <ConnectionError onRetry={() => void refetch()} retrying={isFetching} />
    );
  }

  return <>{children}</>;
}

/** Minimal centered spinner shown while the first reachability probe resolves. */
function ConnectionSplash() {
  return (
    <div className="empty col items-center justify-center">
      <RefreshCw
        size={22}
        strokeWidth={2}
        className="animate-spin"
        aria-label="Connecting to Wardnet"
      />
    </div>
  );
}

interface ConnectionErrorProps {
  onRetry: () => void;
  retrying: boolean;
}

/**
 * Full-screen interstitial shown when the daemon can't be reached on load.
 * Mirrors the 404 page's Forge `.empty` block so the two whole-page states
 * read as a family.
 */
function ConnectionError({ onRetry, retrying }: ConnectionErrorProps) {
  return (
    <div role="alert" className="empty col items-center justify-center gap-4">
      <h2 className="h-title">Can&rsquo;t reach Wardnet</h2>
      <p className="h-sub max-w-md">
        The Wardnet daemon isn&rsquo;t responding. Make sure it&rsquo;s running
        and that you&rsquo;re on the same network, then try again.
      </p>
      <Button variant="outline" onClick={onRetry} disabled={retrying}>
        <RefreshCw
          size={15}
          strokeWidth={2}
          className={retrying ? "animate-spin" : undefined}
        />
        {retrying ? "Retrying…" : "Retry"}
      </Button>
    </div>
  );
}
