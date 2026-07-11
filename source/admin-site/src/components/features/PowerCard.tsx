import { useState } from "react";
import { PowerIcon, RotateCcwIcon, RefreshCwIcon } from "lucide-react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import {
  AlertModal,
  AlertModalAction,
  AlertModalCancel,
  AlertModalContent,
  AlertModalFooter,
  AlertModalTitleBlock,
} from "@wardnet/web";

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
export function PowerCard({
  onReboot,
  onShutdown,
  onRestartDaemon,
  busy,
}: Props) {
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
          <Text as="p" size="sm" className="text-ink-3">
            Reboot the system. Wardnet will be unavailable for ~30-60 seconds
            while it comes back up; managed devices fall back to the upstream
            router during that gap.
          </Text>
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
              data-testid="power-restart-daemon"
            >
              <RefreshCwIcon />
              Restart daemon
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <Text as="p" size="sm" className="text-ink-3">
            Restart only the wardnetd process. The host keeps running. Use this
            if support has asked you to.
          </Text>
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
          <Text as="p" size="sm" className="text-danger-soft-ink">
            Power Wardnet off. Internet for managed devices will go through your
            upstream router until you turn the system back on manually.
          </Text>
        </CardContent>
      </Card>

      <AlertModal open={rebootOpen} onOpenChange={setRebootOpen}>
        <AlertModalContent>
          <AlertModalTitleBlock
            title="Reboot Wardnet?"
            description="Wardnet will restart. Your network will be unavailable for ~30-60 seconds while it comes back up."
          />
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
          <AlertModalTitleBlock
            title="Shut Wardnet down?"
            description="Wardnet will power off. Internet will be unavailable until you turn the system back on manually."
          />
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
          <AlertModalTitleBlock
            title="Restart daemon?"
            description="The wardnetd process will exit and the supervisor will bring it back up. The host keeps running; the network stays online except for a few seconds while the daemon comes back."
          />
          <AlertModalFooter>
            <AlertModalCancel asChild>
              <Button variant="outline">Cancel</Button>
            </AlertModalCancel>
            <AlertModalAction asChild>
              <Button
                data-testid="power-restart-confirm"
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
