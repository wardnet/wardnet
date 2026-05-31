import { useCallback, useRef, useState } from "react";
import { WardnetApiError } from "@wardnet/js";
import { systemService } from "../lib/sdk";

/**
 * Observable state of an in-flight power lifecycle.
 *
 * Shared across `useRestart`, `useReboot`, and `useShutdown`. Each
 * caller maps the terminal phases to its own copy:
 *
 * - `idle` — no power op in progress; the dialog is closed.
 * - `scheduled` — request accepted (HTTP 204). The daemon should
 *   exit shortly. We poll `/api/info` (unauthenticated). While the
 *   process is still alive the probe succeeds, so we stay in this
 *   phase until the first failure.
 * - `down` — one or more consecutive probes failed; the daemon is
 *   either exiting or not yet back up.
 * - `ready` — probe succeeded again *and* the admin cookie still
 *   resolves a valid session. Restart's "all good, continue" state.
 * - `ready_signed_out` — probe succeeded but the session probe
 *   returned 401; the cookie was invalidated by the restart (e.g.
 *   in-memory session store). Operator needs to sign in again.
 * - `off` — daemon went down and stayed down for `OFF_CONFIRM_MS`.
 *   Shutdown's terminal "host is powered off, no recovery" state.
 *   Reboot never resolves to `off` (it's expected to come back).
 * - `did_not_fire` — request returned 204 but the daemon is *still*
 *   reachable `DID_NOT_FIRE_MS` later. Means the spawned task
 *   succeeded the 204 but logind refused — almost always because
 *   the polkit migration hasn't been applied. Surface actionable
 *   copy pointing at Settings → Updates.
 * - `timeout` — the lifecycle didn't reach a definitive terminal
 *   state within `TIMEOUT_MS`. On the dev mock this is the
 *   expected path for restart/reboot (no supervisor brings the
 *   daemon back); on a Pi this means something is wrong.
 * - `failed` — the initial POST itself failed; nothing further.
 */
export type DaemonReachabilityPhase =
  | "idle"
  | "scheduled"
  | "down"
  | "ready"
  | "ready_signed_out"
  | "off"
  | "did_not_fire"
  | "timeout"
  | "failed";

/** Which kind of power op is being observed — controls how the
 *  state machine resolves. */
export type ReachabilityMode =
  | { kind: "restart" } // expects daemon back, validates session
  | { kind: "reboot" } // expects daemon back (host comes back up)
  | { kind: "shutdown" }; // expects daemon to stay down ⇒ `off`

/** Upper bound on the whole lifecycle before we give up. Matches
 *  the existing `useRestart` value so the UX doesn't drift. */
const TIMEOUT_MS = 45_000;
/** Probe interval while waiting for the daemon to come back. */
const POLL_INTERVAL_MS = 800;
/** If the daemon is still reachable this long after a request, we
 *  conclude the spawned task never fired (almost always missing
 *  polkit rule). 8 s = the 500 ms grace window plus a comfortable
 *  margin for systemctl + logind round-trips. */
const DID_NOT_FIRE_MS = 8_000;
/** Shutdown only: number of consecutive-down probes required to
 *  confirm the host is off rather than mid-reboot. ~3 s of silence
 *  is enough to distinguish a true poweroff from a brief socket
 *  blip. */
const OFF_CONFIRM_DOWN_PROBES = 4;

/**
 * Lifecycle state machine for "the daemon is about to go away".
 *
 * Owns the poll loop. Consumers fire the appropriate POST, then
 * call `start()` to enter `scheduled` and let the hook drive the
 * rest of the state machine. `mode` determines how the terminal
 * phases resolve:
 *
 * | Mode        | Daemon reachable post-grace | Came back, session OK | Came back, session 401 | Down for ≥ N probes |
 * | ----------- | --------------------------- | --------------------- | ---------------------- | ------------------- |
 * | `restart`   | did_not_fire                | ready                 | ready_signed_out       | (keeps polling)     |
 * | `reboot`    | did_not_fire                | ready                 | ready_signed_out       | (keeps polling)     |
 * | `shutdown`  | did_not_fire                | (treats as bounce)    | (treats as bounce)     | off                 |
 *
 * The hook does not write to TanStack Query caches.
 */
export function useDaemonReachability() {
  const [phase, setPhase] = useState<DaemonReachabilityPhase>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [startedAt, setStartedAt] = useState<number | null>(null);

  const cancelRef = useRef<(() => void) | null>(null);

  const startPolling = useCallback((mode: ReachabilityMode) => {
    let cancelled = false;
    let seenDown = false;
    let downStreak = 0;
    const startedAtLocal = Date.now();
    const timeoutAt = startedAtLocal + TIMEOUT_MS;
    const didNotFireAt = startedAtLocal + DID_NOT_FIRE_MS;

    cancelRef.current = () => {
      cancelled = true;
    };

    const tick = async () => {
      while (!cancelled) {
        const now = Date.now();
        if (now > timeoutAt) {
          if (!cancelled) setPhase("timeout");
          return;
        }

        let probeOk;
        try {
          const res = await fetch("/api/info", { cache: "no-store" });
          probeOk = res.ok;
        } catch {
          probeOk = false;
        }

        if (!probeOk) {
          seenDown = true;
          downStreak += 1;
          if (mode.kind === "shutdown" && downStreak >= OFF_CONFIRM_DOWN_PROBES) {
            // Daemon has been silent long enough — host is off.
            if (!cancelled) setPhase("off");
            return;
          }
          if (!cancelled) setPhase("down");
        } else {
          downStreak = 0;
          if (seenDown) {
            // Daemon is back. For shutdown, this is unexpected (the
            // host came back up on its own?) — treat the same as a
            // restart bounce: recheck session.
            try {
              await systemService.getStatus();
              if (!cancelled) setPhase("ready");
            } catch (err) {
              if (err instanceof WardnetApiError && err.status === 401) {
                if (!cancelled) setPhase("ready_signed_out");
              } else {
                // Non-auth error (network blip, 5xx). Treat as ready
                // and let the rest of the app surface any real issue.
                if (!cancelled) setPhase("ready");
              }
            }
            return;
          }
          // Still reachable, never went down. If we've blown past
          // the did-not-fire window we conclude logind never accepted
          // the request — most often a missing polkit rule.
          if (now > didNotFireAt) {
            if (!cancelled) setPhase("did_not_fire");
            return;
          }
          // Otherwise stay in `scheduled` and keep polling.
        }

        await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
      }
    };

    void tick();
  }, []);

  /** Move into `scheduled` and start polling for the given mode. */
  const start = useCallback(
    (mode: ReachabilityMode) => {
      setErrorMessage(null);
      setPhase("scheduled");
      setStartedAt(Date.now());
      startPolling(mode);
    },
    [startPolling],
  );

  /** Move directly into `failed` with an error message. Used when
   *  the initial POST itself rejects. Tears down any active poll. */
  const fail = useCallback((message: string) => {
    if (cancelRef.current) cancelRef.current();
    cancelRef.current = null;
    setErrorMessage(message);
    setPhase("failed");
  }, []);

  /** Move into `scheduled` *without* starting the poll loop. Used by
   *  callers that prefer to fire the POST first and only start
   *  polling on a 204 — keeps a 4xx/5xx out of the poll budget. */
  const markScheduled = useCallback(() => {
    setErrorMessage(null);
    setPhase("scheduled");
    setStartedAt(Date.now());
  }, []);

  /** Tear down any in-flight poll and reset to `idle`. */
  const reset = useCallback(() => {
    if (cancelRef.current) cancelRef.current();
    cancelRef.current = null;
    setPhase("idle");
    setStartedAt(null);
    setErrorMessage(null);
  }, []);

  return {
    phase,
    errorMessage,
    startedAt,
    start,
    markScheduled,
    fail,
    reset,
    /** `true` whenever the dialog should be open (any non-idle phase). */
    isOpen: phase !== "idle",
  };
}
