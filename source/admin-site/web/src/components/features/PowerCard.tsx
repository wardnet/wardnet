import { useState } from "react";
import { PowerIcon, RotateCcwIcon, RefreshCwIcon } from "lucide-react";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
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
  /** Confirm dialog → fires `POST /api/system/shutdown`. Destructive action — tinted card. */
  onShutdown: () => void;
  /** Confirm dialog → fires `POST /api/system/restart`. Advanced/secondary lifecycle. */
  onRestartDaemon: () => void;
  /** Disable every trigger while any lifecycle is mid-flight. */
  busy: boolean;
}

/**
 * Power controls on the /power page. Renders three sibling cards —
 * one per lifecycle (Reboot, Restart daemon, Shutdown) — so each
 * action has its own surface. The Shutdown card keeps the standard
 * hairline border but uses a danger-soft background tint to signal
 * the destructive nature of the action.
 *
 * Copy is host-agnostic: Wardnet may run on a Pi, a Docker container,
 * or a Linux server — we say "Wardnet" / "the system" rather than
 * "the Pi".
 */
export function PowerCard({ onReboot, onShutdown, onRestartDaemon, busy }: Props) {
  // Local open-state for each confirmation dialog so the open/close logic
  // stays right next to the button that triggers it.
  const [rebootOpen, setRebootOpen] = useState(false);
  const [shutdownOpen, setShutdownOpen] = useState(false);
  const [restartOpen, setRestartOpen] = useState(false);

  return (
    <div className="flex flex-col gap-4">
      {/* Reboot — non-destructive lifecycle, outline action. */}
      <Card>
        <CardHeader>
          <CardTitle>Reboot</CardTitle>
          <CardAction>
            <Button
              variant="outline"
              onClick={() => setRebootOpen(true)}
              disabled={busy}
              aria-label="Reboot"
            >
              <RotateCcwIcon />
              Reboot
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-ink-3">
            Reboot the system. Wardnet will be unavailable for ~30–60 seconds while it comes back
            up; managed devices fall back to the upstream router during that gap.
          </p>
        </CardContent>
      </Card>

      {/* Restart daemon — advanced; restarts only the wardnetd process. */}
      <Card>
        <CardHeader>
          <CardTitle>Restart daemon</CardTitle>
          <CardAction>
            <Button
              variant="outline"
              onClick={() => setRestartOpen(true)}
              disabled={busy}
              aria-label="Restart daemon"
            >
              <RefreshCwIcon />
              Restart daemon
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-ink-3">
            Restart only the wardnetd process. The host keeps running. Use this if support has asked
            you to.
          </p>
        </CardContent>
      </Card>

      {/* Shutdown — destructive lifecycle. Same hairline border as
          the other cards; only the background tint shifts to mark
          the danger zone. Override via inline style because Tailwind
          utilities sit in @layer utilities and lose to unlayered
          `.card { background: var(--bg-card) }`. */}
      <Card style={{ background: "var(--danger-soft)" }}>
        <CardHeader>
          <CardTitle>Shutdown</CardTitle>
          <CardAction>
            <Button
              variant="destructive"
              onClick={() => setShutdownOpen(true)}
              disabled={busy}
              aria-label="Shutdown"
            >
              <PowerIcon />
              Shut down
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-danger-soft-ink">
            Power Wardnet off. Internet for managed devices will go through your upstream router
            until you turn the system back on manually.
          </p>
        </CardContent>
      </Card>

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
              Wardnet will power off. Internet will be unavailable until you turn the system back on
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
              The wardnetd process will exit and the supervisor will bring it back up. The host
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
