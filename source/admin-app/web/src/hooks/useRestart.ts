import { useCallback } from "react";
import { WardnetApiError } from "@wardnet/js";
import { systemService } from "@/lib/sdk";
import { useDaemonReachability } from "./useDaemonReachability";

/** Phases the restart flow may surface. Subset of
 *  [`DaemonReachabilityPhase`] — restart never resolves to `off`. */
export type RestartPhase =
  | "idle"
  | "scheduled"
  | "down"
  | "ready"
  | "ready_signed_out"
  | "did_not_fire"
  | "timeout"
  | "failed";

/**
 * Lifecycle manager for a daemon restart from the web UI.
 *
 * Thin wrapper over [`useDaemonReachability`] that fires
 * `POST /api/system/restart` and starts the shared poll loop in
 * "restart" mode once the server has accepted the request.
 *
 * Public surface mirrors the previous (pre-#215) implementation —
 * existing callers (Settings page, post-restore prompt) keep
 * working without changes.
 */
export function useRestart() {
  const reach = useDaemonReachability();

  const start = useCallback(() => {
    reach.markScheduled();
    systemService
      .restart()
      .then(() => {
        reach.start({ kind: "restart" });
      })
      .catch((err: unknown) => {
        const msg =
          err instanceof WardnetApiError
            ? (err.body.detail ?? err.body.error)
            : err instanceof Error
              ? err.message
              : "Failed to restart";
        reach.fail(msg);
      });
  }, [reach]);

  return {
    phase: reach.phase as RestartPhase,
    errorMessage: reach.errorMessage,
    startedAt: reach.startedAt,
    isOpen: reach.isOpen,
    start,
    reset: reach.reset,
  };
}
