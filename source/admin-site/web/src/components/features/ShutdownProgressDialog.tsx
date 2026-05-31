import { PowerOffIcon } from "lucide-react";
import { ProgressDialog, type GenericPhase } from "@/components/compound/ProgressDialog";
import type { ShutdownPhase } from "@wardnet/wardnet-web";

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
 * Thin wrapper around the generic [`ProgressDialog`](../compound/ProgressDialog.tsx)
 * compound. Differs from [`RestartProgressDialog`](./RestartProgressDialog.tsx)
 * in that the terminal `off` state is **definitive**: there is no recovery
 * spinner, no retry, and no comeback poll. The operator must physically
 * power the Pi back on. Copy is from the issue's Decision 1.
 *
 * | ShutdownPhase | GenericPhase | Icon / CTA                                  |
 * | ------------- | ------------ | ------------------------------------------- |
 * | scheduled     | in-progress  | spinner — "Shutting down Wardnet"           |
 * | down          | in-progress  | spinner — "Confirming Wardnet is off"       |
 * | off           | success      | power-off — "Wardnet is now off"            |
 * | did_not_fire  | failed       | warning — "Shutdown didn't fire"            |
 * | timeout       | failed       | warning — couldn't confirm host went off    |
 * | failed        | failed       | warning — request rejected                  |
 */
export function ShutdownProgressDialog({ open, phase, startedAt, errorMessage, onDismiss }: Props) {
  return (
    <ProgressDialog
      open={open}
      phase={toGenericPhase(phase)}
      title={titleFor(phase)}
      description={descriptionFor(phase)}
      startedAt={startedAt}
      errorMessage={errorMessage}
      successIcon={<PowerOffIcon className="h-5 w-5 text-ink-3" />}
      successTitle="Wardnet is now off"
      successDescription="Wardnet is now off. Internet for managed devices will go through your home router. To bring Wardnet back, power your Pi on and refresh this page."
      onDismiss={onDismiss}
    />
  );
}

function toGenericPhase(phase: ShutdownPhase): GenericPhase {
  switch (phase) {
    case "scheduled":
    case "down":
      return "in-progress";
    case "off":
      return "success";
    case "did_not_fire":
    case "timeout":
    case "failed":
      return "failed";
    default:
      return "in-progress";
  }
}

function titleFor(phase: ShutdownPhase): string {
  switch (phase) {
    case "scheduled":
      return "Shutting down Wardnet";
    case "down":
      return "Confirming Wardnet is off";
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

function descriptionFor(phase: ShutdownPhase): string {
  switch (phase) {
    case "scheduled":
      return "Waiting for the daemon to exit.";
    case "down":
      return "Daemon went offline. Confirming the host is fully off.";
    case "did_not_fire":
      return "Wardnet didn't shut down. The privileged migration may not have run — try Settings → Updates, or re-run install.sh.";
    case "timeout":
      return "The daemon went offline but came back before we could confirm the host had powered off. Try again.";
    default:
      return "";
  }
}
