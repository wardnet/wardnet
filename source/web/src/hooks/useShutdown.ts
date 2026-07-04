import { useCallback } from "react";
import { WardnetApiError } from "@wardnet/js";
import { systemService } from "../lib/sdk";
import { useDaemonReachability } from "./useDaemonReachability";

/** Phases the shutdown flow may surface. Resolves to `off` (the
 *  host is powered down) instead of `ready`/`ready_signed_out`. */
export type ShutdownPhase =
  "idle" | "scheduled" | "down" | "off" | "did_not_fire" | "timeout" | "failed";

/**
 * Lifecycle manager for a host shutdown from the web UI.
 *
 * Fires `POST /api/system/shutdown`, then runs the shared
 * [`useDaemonReachability`] state machine in "shutdown" mode. The
 * difference vs reboot/restart: the lifecycle resolves to a
 * **terminal** `off` once the daemon has been silent for a few
 * consecutive probes — the operator must manually power the Pi
 * back on, so there is no comeback poll and the dialog copy says
 * exactly that.
 */
export function useShutdown() {
  const reach = useDaemonReachability();

  const start = useCallback(() => {
    reach.markScheduled();
    systemService
      .shutdown()
      .then(() => {
        reach.start({ kind: "shutdown" });
      })
      .catch((err: unknown) => {
        const msg =
          err instanceof WardnetApiError
            ? (err.body.detail ?? err.body.error)
            : err instanceof Error
              ? err.message
              : "Failed to shut down";
        reach.fail(msg);
      });
  }, [reach]);

  return {
    phase: reach.phase as ShutdownPhase,
    errorMessage: reach.errorMessage,
    startedAt: reach.startedAt,
    isOpen: reach.isOpen,
    start,
    reset: reach.reset,
  };
}
