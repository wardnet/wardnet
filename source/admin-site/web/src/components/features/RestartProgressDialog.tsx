import type { ReactNode } from "react";
import { CheckCircle2Icon, LogInIcon } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { AlertModalAction } from "@wardnet/forge-web/alert-modal";
import { ProgressDialog, type GenericPhase } from "@/components/compound/ProgressDialog";
import type { RestartPhase } from "@wardnet/wardnet-web";

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
 * Thin wrapper around the generic [`ProgressDialog`](../compound/ProgressDialog.tsx)
 * compound. Maps the richer [`RestartPhase`](../../hooks/useRestart.ts) enum into
 * the three-bucket `GenericPhase` machine and supplies copy/icons per phase:
 *
 * | RestartPhase     | GenericPhase | Icon / CTA                            |
 * | ---------------- | ------------ | ------------------------------------- |
 * | scheduled        | in-progress  | spinner — "Restarting daemon"         |
 * | down             | in-progress  | spinner — "Waiting for daemon…"       |
 * | ready            | success      | check — Continue                      |
 * | ready_signed_out | success      | login — Sign in again                 |
 * | timeout          | failed       | warning — "Daemon didn't come back"   |
 * | failed           | failed       | warning — request rejected            |
 * | did_not_fire     | failed       | warning — "Restart didn't fire"       |
 */
export function RestartProgressDialog({
  open,
  phase,
  startedAt,
  errorMessage,
  onDismiss,
  onSignIn,
}: Props) {
  const generic = toGenericPhase(phase);
  const { title, description, successIcon, successTitle, successDescription, successAction } =
    contentFor(phase, onSignIn);

  return (
    <ProgressDialog
      open={open}
      phase={generic}
      title={title}
      description={description}
      startedAt={startedAt}
      errorMessage={errorMessage}
      successIcon={successIcon}
      successTitle={successTitle}
      successDescription={successDescription}
      successAction={successAction}
      onDismiss={onDismiss}
    />
  );
}

function toGenericPhase(phase: RestartPhase): GenericPhase {
  switch (phase) {
    case "scheduled":
    case "down":
      return "in-progress";
    case "ready":
    case "ready_signed_out":
      return "success";
    case "timeout":
    case "failed":
    case "did_not_fire":
      return "failed";
    default:
      return "in-progress";
  }
}

interface PhaseContent {
  title: string;
  description: ReactNode;
  successIcon: ReactNode;
  successTitle: string;
  successDescription?: ReactNode;
  successAction?: ReactNode;
}

function contentFor(phase: RestartPhase, onSignIn: () => void): PhaseContent {
  // `ready` is the happy path; `ready_signed_out` reuses the same shell
  // but swaps the icon to login + amber and offers a Sign-in CTA.
  const signedOut = phase === "ready_signed_out";

  return {
    title: titleFor(phase),
    description: descriptionFor(phase),
    successIcon: signedOut ? (
      <LogInIcon className="h-5 w-5 text-amber-600" />
    ) : (
      <CheckCircle2Icon className="h-5 w-5 text-green-600" />
    ),
    successTitle: signedOut ? "Daemon restarted, session expired" : "Daemon is back online",
    successDescription: signedOut
      ? "Service is back on, but your session cookie was invalidated. Sign in again to continue."
      : "Service is back on and your session is still valid.",
    successAction: signedOut ? (
      <AlertModalAction asChild>
        <Button onClick={onSignIn}>
          <LogInIcon className="mr-2 h-4 w-4" />
          Sign in again
        </Button>
      </AlertModalAction>
    ) : undefined,
  };
}

function titleFor(phase: RestartPhase): string {
  switch (phase) {
    case "scheduled":
      return "Restarting daemon";
    case "down":
      return "Waiting for daemon to come back";
    case "timeout":
      return "Daemon didn't come back";
    case "failed":
      return "Restart request failed";
    case "did_not_fire":
      return "Restart didn't fire";
    default:
      return "Restart";
  }
}

function descriptionFor(phase: RestartPhase): string {
  switch (phase) {
    case "scheduled":
      return "Waiting for the service to exit.";
    case "down":
      return "Service is no longer responsive, restart in progress.";
    case "timeout":
      return "The daemon didn't come back within 45 seconds. This usually means the supervisor (systemd) needs attention.";
    case "did_not_fire":
      return "Wardnet didn't restart. The privileged migration may not have run — try Settings → Updates, or re-run install.sh.";
    default:
      return "";
  }
}
