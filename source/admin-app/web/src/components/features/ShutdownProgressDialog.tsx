import { useEffect, useState } from "react";
import { Loader2Icon, AlertTriangleIcon, PowerOffIcon } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import {
  AlertModal,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/forge-web/alert-modal";
import type { ShutdownPhase } from "@/hooks/useShutdown";

interface Props {
  /** Whether to render the dialog at all — mirrors `useShutdown().isOpen`. */
  open: boolean;
  /** Current lifecycle phase. */
  phase: ShutdownPhase;
  /** UTC ms timestamp the shutdown was initiated, for elapsed display. */
  startedAt: number | null;
  /** Error message to show in the `failed` phase. */
  errorMessage: string | null;
  /** Called when the operator dismisses the modal (only valid in terminal phases). */
  onDismiss: () => void;
}

/**
 * Full-screen-overlay progress modal for a host shutdown.
 *
 * Differs from [`RestartProgressDialog`] in that the terminal `off`
 * state is **definitive**: there is no recovery spinner, no retry,
 * and no comeback poll. The operator must physically power the Pi
 * back on. Copy is from the issue's Decision 1.
 *
 * | Phase           | Icon       | Message                                            |
 * | --------------- | ---------- | -------------------------------------------------- |
 * | scheduled       | spinner    | "Waiting for the daemon to exit…"                  |
 * | down            | spinner    | "Daemon went offline. Confirming the host is off…" |
 * | off             | poweroff   | "Wardnet is now off." + Dismiss                    |
 * | did_not_fire    | warning    | "Wardnet didn't shut down." + Dismiss              |
 * | timeout         | warning    | "Couldn't confirm the host went off." + Dismiss    |
 * | failed          | warning    | request never accepted — show error + Dismiss      |
 */
export function ShutdownProgressDialog({ open, phase, startedAt, errorMessage, onDismiss }: Props) {
  const elapsed = useElapsedSeconds(open ? startedAt : null);

  const terminal =
    phase === "off" || phase === "did_not_fire" || phase === "timeout" || phase === "failed";

  return (
    <AlertModal
      open={open}
      onOpenChange={(next) => {
        if (!next && terminal) onDismiss();
      }}
    >
      <AlertModalContent>
        <AlertModalHeader>
          <AlertModalTitle className="flex items-center gap-2">
            <PhaseIcon phase={phase} />
            {titleFor(phase)}
          </AlertModalTitle>
          <AlertModalDescription>{descriptionFor(phase, errorMessage)}</AlertModalDescription>
        </AlertModalHeader>

        {!terminal && (
          <div className="text-xs text-muted-foreground">
            Elapsed: {elapsed}s (times out at 45s).
          </div>
        )}

        <AlertModalFooter>
          {terminal && (
            <AlertModalCancel asChild>
              <Button variant="outline" onClick={onDismiss}>
                Dismiss
              </Button>
            </AlertModalCancel>
          )}
        </AlertModalFooter>
      </AlertModalContent>
    </AlertModal>
  );
}

function PhaseIcon({ phase }: { phase: ShutdownPhase }) {
  switch (phase) {
    case "scheduled":
    case "down":
      return <Loader2Icon className="h-5 w-5 animate-spin text-muted-foreground" />;
    case "off":
      return <PowerOffIcon className="h-5 w-5 text-muted-foreground" />;
    case "did_not_fire":
    case "timeout":
    case "failed":
      return <AlertTriangleIcon className="h-5 w-5 text-destructive" />;
    default:
      return null;
  }
}

function titleFor(phase: ShutdownPhase): string {
  switch (phase) {
    case "scheduled":
      return "Shutting down Wardnet";
    case "down":
      return "Confirming Wardnet is off";
    case "off":
      return "Wardnet is now off";
    case "did_not_fire":
      return "Shutdown didn't fire";
    case "timeout":
      return "Couldn't confirm Wardnet went off";
    case "failed":
      return "Shutdown request failed";
    default:
      return "Shutdown";
  }
}

function descriptionFor(phase: ShutdownPhase, errorMessage: string | null): string {
  switch (phase) {
    case "scheduled":
      return "Waiting for the daemon to exit.";
    case "down":
      return "Daemon went offline. Confirming the host is fully off.";
    case "off":
      return "Wardnet is now off. Internet for managed devices will go through your home router. To bring Wardnet back, power your Pi on and refresh this page.";
    case "did_not_fire":
      return "Wardnet didn't shut down. The privileged migration may not have run — try Settings → Updates, or re-run install.sh.";
    case "timeout":
      return "The daemon went offline but came back before we could confirm the host had powered off. Try again.";
    case "failed":
      return errorMessage ?? "The shutdown request itself was rejected.";
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
