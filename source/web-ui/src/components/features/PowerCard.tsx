import { useState } from "react";
import { PowerIcon, RotateCcwIcon, RefreshCwIcon } from "lucide-react";
import { Button } from "@/components/core/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
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
 * inlined here using `core/ui/alert-dialog` (per Decision 5 — don't
 * speculatively extract a generic compound until a second feature
 * needs the same shape).
 *
 * Visual hierarchy (top-to-bottom = most-to-least common usage):
 *   1. Safe Reboot — primary button, default variant
 *   2. Safe Shutdown — destructive (red) variant
 *   3. Restart daemon — small/secondary, lower in the card
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
          <Button onClick={() => setRebootOpen(true)} disabled={busy} aria-label="Safe Reboot">
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

      <AlertDialog open={rebootOpen} onOpenChange={setRebootOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Reboot Wardnet?</AlertDialogTitle>
            <AlertDialogDescription>
              Wardnet will restart. Your network will be unavailable for ~30–60 seconds while it
              comes back up.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setRebootOpen(false);
                onReboot();
              }}
            >
              Reboot
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={shutdownOpen} onOpenChange={setShutdownOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Shut Wardnet down?</AlertDialogTitle>
            <AlertDialogDescription>
              Wardnet will power off. Internet will be unavailable until you turn the Pi back on
              manually.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                setShutdownOpen(false);
                onShutdown();
              }}
            >
              Shut down
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={restartOpen} onOpenChange={setRestartOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restart daemon?</AlertDialogTitle>
            <AlertDialogDescription>
              The wardnetd process will exit and the supervisor will bring it back up. The Pi itself
              keeps running; the network stays online except for a few seconds while the daemon
              comes back.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setRestartOpen(false);
                onRestartDaemon();
              }}
            >
              Restart
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
