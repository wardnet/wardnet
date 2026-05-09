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
  /** Confirm dialog → fires `POST /api/system/reboot`. Primary action. */
  onReboot: () => void;
  /** Confirm dialog → fires `POST /api/system/shutdown`. Destructive action. */
  onShutdown: () => void;
  /** Confirm dialog → fires `POST /api/system/restart`. Advanced/secondary. */
  onRestartDaemon: () => void;
  /** Disable all three buttons while any of the lifecycles is mid-flight. */
  busy: boolean;
}

/**
 * Power controls card on the Settings page.
 *
 * Pure presentation: receives three callbacks and a `busy` flag, has
 * no idea TanStack Query or the SDK exist. Confirmation dialogs are
 * inlined here using `@wardnet/forge-web/alert-modal` (per Decision 5 —
 * don't speculatively extract a generic compound until a second feature
 * needs the same shape).
 *
 * Visual hierarchy: all three are secondary actions — none is a
 * happy-path CTA, so they all use the `outline` variant. Shutdown
 * keeps the destructive (red) styling because it's the only one
 * that leaves the network without internet until the operator
 * physically powers the Pi back on.
 */
export function PowerCard({ onReboot, onShutdown, onRestartDaemon, busy }: Props) {
  // The two confirmation dialogs live as local state so we can keep
  // the open/close logic right next to the corresponding button.
  const [rebootOpen, setRebootOpen] = useState(false);
  const [shutdownOpen, setShutdownOpen] = useState(false);
  const [restartOpen, setRestartOpen] = useState(false);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Power</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {/* Safe Reboot — primary, default variant. */}
        <div className="flex items-center justify-between border-b pb-3">
          <div>
            <div className="text-sm font-medium">Safe Reboot</div>
            <div className="text-xs text-muted-foreground">
              Reboot the Pi. Wardnet will be unavailable for ~30–60 seconds while it comes back up;
              managed devices fall back to the upstream router during that gap.
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

        {/* Safe Shutdown — destructive variant. */}
        <div className="flex items-center justify-between border-b pb-3">
          <div>
            <div className="text-sm font-medium">Safe Shutdown</div>
            <div className="text-xs text-muted-foreground">
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

        {/* Restart daemon — small / secondary, advanced. */}
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium">Restart daemon (advanced)</div>
            <div className="text-xs text-muted-foreground">
              Restart only the wardnetd process. The Pi keeps running. Use this if support has asked
              you to.
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
    </Card>
  );
}
