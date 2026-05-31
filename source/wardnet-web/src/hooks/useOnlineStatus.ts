import { useEffect, useState } from "react";

const DEFAULT_PROBE_INTERVAL_MS = 15_000;

/**
 * Tracks two independent dimensions of "reachability":
 *
 * 1. **Browser online/offline** — derived from `navigator.onLine` and the
 *    `online`/`offline` window events. This goes `false` when the device
 *    loses its network interface entirely; it does NOT reliably indicate
 *    that the daemon is reachable.
 *
 * 2. **Daemon reachability** — a lightweight periodic `GET /api/info` probe.
 *    Goes `false` when the request fails or returns a non-2xx status (e.g.
 *    the device's IP changed, the daemon process is down, or the user is on
 *    a different network). Starts `true` optimistically; the first probe
 *    result arrives within one tick.
 *
 * Use `showingLastKnownState` to drive the "offline — showing last known state"
 * banner: it is `true` whenever either dimension reports unreachable.
 */
export function useOnlineStatus(options?: {
  /** How often to probe daemon reachability in milliseconds. Default: 15 000. */
  daemonProbeIntervalMs?: number;
}) {
  const daemonProbeIntervalMs = options?.daemonProbeIntervalMs ?? DEFAULT_PROBE_INTERVAL_MS;

  const [isOnline, setIsOnline] = useState(() => navigator.onLine);
  const [isDaemonReachable, setIsDaemonReachable] = useState(true);

  useEffect(() => {
    const handleOnline = () => setIsOnline(true);
    const handleOffline = () => setIsOnline(false);
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
    };
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    let timeoutId: number | undefined;

    const probe = async () => {
      try {
        const res = await fetch("/api/info", { cache: "no-store", signal: controller.signal });
        if (!cancelled) setIsDaemonReachable(res.ok);
      } catch (e) {
        // Ignore AbortError — it means the component unmounted or the interval changed.
        if (!cancelled && !(e instanceof DOMException && e.name === "AbortError")) {
          setIsDaemonReachable(false);
        }
      }
    };

    // Chained setTimeout rather than setInterval: the next probe is only scheduled
    // after the previous one resolves, preventing concurrent in-flight requests when
    // the network is slow or the daemon is unreachable.
    const schedule = async () => {
      await probe();
      if (!cancelled) {
        timeoutId = window.setTimeout(() => void schedule(), daemonProbeIntervalMs);
      }
    };

    void schedule();

    return () => {
      cancelled = true;
      controller.abort();
      clearTimeout(timeoutId);
    };
  }, [daemonProbeIntervalMs]);

  return {
    /** `true` when `navigator.onLine` reports network connectivity. */
    isOnline,
    /** `true` when a recent `/api/info` probe succeeded. */
    isDaemonReachable,
    /** `true` when the app is operating on stale cached data. Drives the offline banner. */
    showingLastKnownState: !isOnline || !isDaemonReachable,
  };
}
