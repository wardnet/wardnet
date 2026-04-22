import { useCallback, useRef, useState } from "react";
import { WardnetApiError } from "@wardnet/js";
import { systemService } from "@/lib/sdk";

/**
 * Observable state of an in-flight daemon restart.
 *
 * The dialog surfaces phase-specific copy so the operator can tell
 * what's happening at each step:
 *
 * - `idle` — no restart in progress; the dialog is closed.
 * - `scheduled` — server accepted `POST /api/system/restart` and the
 *   daemon is about to exit. We keep polling `/api/info`; while the
 *   process is still alive the probe succeeds, so we stay in this
 *   phase until the first failure.
 * - `down` — one or more consecutive probes failed; the daemon is
 *   either exiting or not yet back up.
 * - `ready` — probe succeeded again *and* the admin cookie still
 *   resolves a valid session. Operator can dismiss the dialog and
 *   continue using the app.
 * - `ready_signed_out` — probe succeeded but the session probe
 *   returned 401; the cookie was invalidated by the restart (e.g.
 *   in-memory session store). Operator needs to sign in again.
 * - `timeout` — the daemon didn't come back within `TIMEOUT_MS`.
 *   On the dev mock this is the expected path (no supervisor); on
 *   a Pi this means something is wrong and the operator must
 *   intervene manually.
 * - `failed` — the initial `POST /api/system/restart` itself
 *   failed; nothing further is done.
 */
export type RestartPhase =
  | "idle"
  | "scheduled"
  | "down"
  | "ready"
  | "ready_signed_out"
  | "timeout"
  | "failed";

/** Upper bound on the whole restart cycle before we give up. */
const TIMEOUT_MS = 45_000;
/** Probe interval while waiting for the daemon to come back. */
const POLL_INTERVAL_MS = 800;

/**
 * Lifecycle manager for a daemon restart from the web UI.
 *
 * Models the full restart cycle — schedule, wait, detect down,
 * detect back, verify auth — as a phase state machine. Components
 * render phase-specific copy; the hook owns the polling loop.
 *
 * Nothing here writes to TanStack Query caches: downstream queries
 * discover the daemon is back through their own refetch logic as
 * soon as the next poll succeeds.
 */
export function useRestart() {
  const [phase, setPhase] = useState<RestartPhase>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [startedAt, setStartedAt] = useState<number | null>(null);

  const cancelRef = useRef<(() => void) | null>(null);

  /** Poll `/api/info` (unauthenticated) until the daemon returns. */
  const startPolling = useCallback(() => {
    let cancelled = false;
    let seenDown = false;
    const startedAtLocal = Date.now();
    const timeoutAt = startedAtLocal + TIMEOUT_MS;

    cancelRef.current = () => {
      cancelled = true;
    };

    const tick = async () => {
      while (!cancelled) {
        if (Date.now() > timeoutAt) {
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
          if (!cancelled) setPhase("down");
        } else if (seenDown) {
          // Daemon is back — verify the session survived.
          try {
            await systemService.getStatus();
            if (!cancelled) setPhase("ready");
          } catch (err) {
            if (err instanceof WardnetApiError && err.status === 401) {
              if (!cancelled) setPhase("ready_signed_out");
            } else {
              // Non-auth error (network blip, 5xx). Treat as ready
              // and let the rest of the app surface any real failure.
              if (!cancelled) setPhase("ready");
            }
          }
          return;
        }
        // If `probeOk` and we haven't seen a down yet, the daemon
        // just hasn't exited yet. Keep polling in `scheduled`.

        await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
      }
    };

    void tick();
  }, []);

  const start = useCallback(() => {
    setErrorMessage(null);
    setPhase("scheduled");
    setStartedAt(Date.now());

    systemService
      .restart()
      .then(() => {
        startPolling();
      })
      .catch((err: unknown) => {
        const msg =
          err instanceof WardnetApiError
            ? (err.body.detail ?? err.body.error)
            : err instanceof Error
              ? err.message
              : "Failed to restart";
        setErrorMessage(msg);
        setPhase("failed");
      });
  }, [startPolling]);

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
    reset,
    /** `true` whenever the dialog should be open (any non-idle phase). */
    isOpen: phase !== "idle",
  };
}
