import { useState } from "react";
import { PowerIcon, RotateCcwIcon, RefreshCwIcon } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import {
  AlertModal,
  AlertModalAction,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/forge-web/alert-modal";

interface Props {
  /** Confirm dialog → fires `POST /api/system/reboot`. Non-destructive lifecycle. */
  onReboot: () => void;
  /** Confirm dialog → fires `POST /api/system/shutdown`. Destructive — lives outside the card. */
  onShutdown: () => void;
  /** Confirm dialog → fires `POST /api/system/restart`. Advanced/secondary lifecycle. */
  onRestartDaemon: () => void;
  /** Disable every trigger (in-card and outside-card) while any lifecycle is mid-flight. */
  busy: boolean;
}

/**
 * Power controls block on the Settings page.
 *
 * Pure presentation: receives three callbacks and a `busy` flag, has
 * no idea TanStack Query or the SDK exist. Confirmation dialogs are
 * inlined here using `@wardnet/forge-web/alert-modal` (per Decision 5 —
 * don't speculatively extract a generic compound until a second feature
 * needs the same shape).
 *
 * Layout follows the "danger-toned actions outside card" rule from the
 * design-system §detail skill — non-destructive lifecycles (Reboot,
 * Restart daemon) live inside the `Power` card; the destructive
 * Shutdown sits outside the card in a danger-toned row below it.
 */
export function PowerCard({ onReboot, onShutdown, onRestartDaemon, busy }: Props) {
  // Local open-state for each confirmation dialog so the open/close logic
  // stays right next to the button that triggers it.
  const [rebootOpen, setRebootOpen] = useState(false);
  const [shutdownOpen, setShutdownOpen] = useState(false);
  const [restartOpen, setRestartOpen] = useState(false);

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Power</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {/* Safe Reboot — non-destructive lifecycle, outline variant. */}
          <div className="flex items-center justify-between border-b pb-3">
            <div>
              <div className="text-sm font-medium">Safe Reboot</div>
              <div className="text-xs text-ink-3">
                Reboot the Pi. Wardnet will be unavailable for ~30–60 seconds while it comes back
                up; managed devices fall back to the upstream router during that gap.
              </div>
            </div>
            <Button
              variant="outline"
              onClick={() => setRebootOpen(true)}
              disabled={busy}
              aria-label="Safe Reboot"
            >
              <RotateCcwIcon />
              Reboot
            </Button>
          </div>

          {/* Restart daemon — advanced, secondary, outline variant. */}
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium">Restart daemon (advanced)</div>
              <div className="text-xs text-ink-3">
                Restart only the wardnetd process. The Pi keeps running. Use this if support has
                asked you to.
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setRestartOpen(true)}
              disabled={busy}
              aria-label="Restart daemon"
            >
              <RefreshCwIcon />
              Restart daemon
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Safe Shutdown — destructive action, outside the card per skill §detail. */}
      <div className="flex items-center justify-between gap-4 rounded-md border border-danger-soft bg-danger-soft px-4 py-3">
        <div>
          <div className="text-sm font-medium text-danger-soft-ink">Safe Shutdown</div>
          <div className="text-xs text-ink-3">
            Power the Pi off. Internet for managed devices will go through your home router until
            you turn the Pi back on manually.
          </div>
        </div>
        <Button
          variant="destructive"
          onClick={() => setShutdownOpen(true)}
          disabled={busy}
          aria-label="Safe Shutdown"
        >
          <PowerIcon />
          Shut down
        </Button>
      </div>

      <AlertModal open={rebootOpen} onOpenChange={setRebootOpen}>
        <AlertModalContent>
          <AlertModalHeader>
            <AlertModalTitle>Reboot Wardnet?</AlertModalTitle>
            <AlertModalDescription>
              Wardnet will restart. Your network will be unavailable for ~30–60 seconds while it
              comes back up.
            </AlertModalDescription>
          </AlertModalHeader>
          <AlertModalFooter>
            <AlertModalCancel asChild>
              <Button variant="outline">Cancel</Button>
            </AlertModalCancel>
            <AlertModalAction asChild>
              <Button
                onClick={() => {
                  setRebootOpen(false);
                  onReboot();
                }}
              >
                Reboot
              </Button>
            </AlertModalAction>
          </AlertModalFooter>
        </AlertModalContent>
      </AlertModal>

      <AlertModal open={shutdownOpen} onOpenChange={setShutdownOpen}>
        <AlertModalContent>
          <AlertModalHeader>
            <AlertModalTitle>Shut Wardnet down?</AlertModalTitle>
            <AlertModalDescription>
              Wardnet will power off. Internet will be unavailable until you turn the Pi back on
              manually.
            </AlertModalDescription>
          </AlertModalHeader>
          <AlertModalFooter>
            <AlertModalCancel asChild>
              <Button variant="outline">Cancel</Button>
            </AlertModalCancel>
            <AlertModalAction asChild>
              <Button
                variant="destructive"
                onClick={() => {
                  setShutdownOpen(false);
                  onShutdown();
                }}
              >
                Shut down
              </Button>
            </AlertModalAction>
          </AlertModalFooter>
        </AlertModalContent>
      </AlertModal>

      <AlertModal open={restartOpen} onOpenChange={setRestartOpen}>
        <AlertModalContent>
          <AlertModalHeader>
            <AlertModalTitle>Restart daemon?</AlertModalTitle>
            <AlertModalDescription>
              The wardnetd process will exit and the supervisor will bring it back up. The Pi itself
              keeps running; the network stays online except for a few seconds while the daemon
              comes back.
            </AlertModalDescription>
          </AlertModalHeader>
          <AlertModalFooter>
            <AlertModalCancel asChild>
              <Button variant="outline">Cancel</Button>
            </AlertModalCancel>
            <AlertModalAction asChild>
              <Button
                onClick={() => {
                  setRestartOpen(false);
                  onRestartDaemon();
                }}
              >
                Restart
              </Button>
            </AlertModalAction>
          </AlertModalFooter>
        </AlertModalContent>
      </AlertModal>
    </div>
  );
}
