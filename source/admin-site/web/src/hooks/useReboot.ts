import { useCallback } from "react";
import { WardnetApiError } from "@wardnet/js";
import { systemService } from "@/lib/sdk";
import { useDaemonReachability } from "./useDaemonReachability";

/** Phases the reboot flow may surface. Same as
 *  [`useRestart`](./useRestart.ts) — both expect the daemon back. */
export type RebootPhase =
  | "idle"
  | "scheduled"
  | "down"
  | "ready"
  | "ready_signed_out"
  | "did_not_fire"
  | "timeout"
  | "failed";

/**
 * Lifecycle manager for a host reboot from the web UI.
 *
 * Fires `POST /api/system/reboot`, then runs the shared
 * [`useDaemonReachability`] state machine in "reboot" mode. The
 * lifecycle is the same shape as `useRestart` (the daemon is
 * expected to come back); the difference is the trigger endpoint
 * and the on-the-wire effect — see #213's GARP failover, which
 * uses this endpoint as its e2e trigger.
 */
export function useReboot() {
  const reach = useDaemonReachability();

  const start = useCallback(() => {
    reach.markScheduled();
    systemService
      .reboot()
      .then(() => {
        reach.start({ kind: "reboot" });
      })
      .catch((err: unknown) => {
        const msg =
          err instanceof WardnetApiError
            ? (err.body.detail ?? err.body.error)
            : err instanceof Error
              ? err.message
              : "Failed to reboot";
        reach.fail(msg);
      });
  }, [reach]);

  return {
    phase: reach.phase as RebootPhase,
    errorMessage: reach.errorMessage,
    startedAt: reach.startedAt,
    isOpen: reach.isOpen,
    start,
    reset: reach.reset,
  };
}
