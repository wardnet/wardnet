import { useEffect, useState } from "react";
import { Loader2Icon, CheckCircle2Icon, AlertTriangleIcon, LogInIcon } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/core/ui/alert-dialog";
import type { RestartPhase } from "@/hooks/useRestart";

interface Props {
  /** Whether to render the dialog at all — mirrors `useRestart().isOpen`. */
  open: boolean;
  /** Current lifecycle phase. */
  phase: RestartPhase;
  /** UTC ms timestamp the restart was initiated, for elapsed display. */
  startedAt: number | null;
  /** Error message to show in the `failed` phase. */
  errorMessage: string | null;
  /** Called when the operator dismisses the modal (only valid in terminal phases). */
  onDismiss: () => void;
  /** Called when the operator clicks "Sign in again" in `ready_signed_out`. */
  onSignIn: () => void;
}

/**
 * Full-screen-overlay progress modal for a daemon restart.
 *
 * Phase copy maps 1:1 with [`RestartPhase`](../../hooks/useRestart.ts):
 *
 * | Phase              | Icon     | Message                                            |
 * | ------------------ | -------- | -------------------------------------------------- |
 * | scheduled          | spinner  | "Waiting for the daemon to exit…"                  |
 * | down               | spinner  | "Daemon is offline. Waiting for it to come back…"  |
 * | ready              | check    | "Daemon is back online." + Continue button         |
 * | ready_signed_out   | login    | "Daemon is back, session expired." + Sign in btn   |
 * | timeout            | warning  | "Daemon didn't come back." + Dismiss               |
 * | failed             | warning  | request never accepted — show error + Dismiss      |
 *
 * Only the three terminal phases (`ready`, `ready_signed_out`,
 * `timeout`, `failed`) show a close/action button. The two in-flight
 * phases (`scheduled`, `down`) render a spinner with no escape —
 * the modal is intentionally modal. Radix `AlertDialog` already
 * dims the background and traps focus; no further CSS needed.
 */
export function RestartProgressDialog({
  open,
  phase,
  startedAt,
  errorMessage,
  onDismiss,
  onSignIn,
}: Props) {
  const elapsed = useElapsedSeconds(open ? startedAt : null);

  const terminal =
    phase === "ready" || phase === "ready_signed_out" || phase === "timeout" || phase === "failed";

  return (
    <AlertDialog
      open={open}
      // Only allow dismissal in terminal phases; swallow escape/outside
      // clicks while the daemon is mid-restart.
      onOpenChange={(next) => {
        if (!next && terminal) onDismiss();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <PhaseIcon phase={phase} />
            {titleFor(phase)}
          </AlertDialogTitle>
          <AlertDialogDescription>{descriptionFor(phase, errorMessage)}</AlertDialogDescription>
        </AlertDialogHeader>

        {!terminal && (
          <div className="text-xs text-muted-foreground">
            Elapsed: {elapsed}s (times out at 45s).
          </div>
        )}

        <AlertDialogFooter>
          {phase === "ready" && <AlertDialogAction onClick={onDismiss}>Continue</AlertDialogAction>}
          {phase === "ready_signed_out" && (
            <AlertDialogAction onClick={onSignIn}>
              <LogInIcon className="mr-2 h-4 w-4" />
              Sign in again
            </AlertDialogAction>
          )}
          {(phase === "timeout" || phase === "failed") && (
            <AlertDialogCancel onClick={onDismiss}>Dismiss</AlertDialogCancel>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function PhaseIcon({ phase }: { phase: RestartPhase }) {
  switch (phase) {
    case "scheduled":
    case "down":
      return <Loader2Icon className="h-5 w-5 animate-spin text-muted-foreground" />;
    case "ready":
      return <CheckCircle2Icon className="h-5 w-5 text-green-600" />;
    case "ready_signed_out":
      return <LogInIcon className="h-5 w-5 text-amber-600" />;
    case "timeout":
    case "failed":
      return <AlertTriangleIcon className="h-5 w-5 text-destructive" />;
    default:
      return null;
  }
}

function titleFor(phase: RestartPhase): string {
  switch (phase) {
    case "scheduled":
      return "Restarting daemon";
    case "down":
      return "Waiting for daemon to come back";
    case "ready":
      return "Daemon is back online";
    case "ready_signed_out":
      return "Daemon restarted, session expired";
    case "timeout":
      return "Daemon didn't come back";
    case "failed":
      return "Restart request failed";
    default:
      return "Restart";
  }
}

function descriptionFor(phase: RestartPhase, errorMessage: string | null): string {
  switch (phase) {
    case "scheduled":
      return "Waiting for the service to exit.";
    case "down":
      return "Service is no longer responsive, restart in progress.";
    case "ready":
      return "Service is back on and your session is still valid.";
    case "ready_signed_out":
      return "Service is back on, but your session cookie was invalidated. Sign in again to continue.";
    case "timeout":
      return "The daemon didn't come back within 45 seconds. This usually means the supervisor (systemd) needs attention.";
    case "failed":
      return errorMessage ?? "The restart request itself was rejected.";
    default:
      return "";
  }
}

/** Live-updating elapsed-seconds counter, paused while `startedAt` is null. */
function useElapsedSeconds(startedAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAt === null) return;
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [startedAt]);
  return startedAt === null ? 0 : Math.max(0, Math.floor((now - startedAt) / 1000));
}
