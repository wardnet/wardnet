import { useEffect } from "react";
import { dnsLogStreamService } from "@wardnet/wardnet-web";
import { useDnsLogStore } from "@/stores/dnsLogStore";
import { useAuth } from "@wardnet/wardnet-web";

/**
 * Global DNS log stream connection manager.
 *
 * Renders nothing — mount once at the app root next to LogStreamManager.
 * Only connects for admin sessions: the WS endpoint is admin-gated server-
 * side, and connecting from non-admin sessions just produces a 401 noise
 * loop. The connection is *unfiltered* — every event lands in the Zustand
 * store, and consumers narrow them at render time. Filtering client-side
 * keeps the live tail responsive when the user widens or clears a
 * filter (the server would otherwise have dropped the previously
 * non-matching events).
 */
export function DnsLogStreamManager() {
  const { isAdmin } = useAuth();
  const store = useDnsLogStore;

  useEffect(() => {
    if (!isAdmin) return;

    dnsLogStreamService.connect({
      onEvent: (event) => store.getState()._addEvent(event),
      onLagged: (skipped) => store.getState()._addSkipped(skipped),
      onConnected: () => store.getState()._setConnected(true),
      onDisconnected: () => store.getState()._setConnected(false),
    });

    return () => {
      dnsLogStreamService.disconnect();
    };
  }, [isAdmin, store]);

  return null;
}
