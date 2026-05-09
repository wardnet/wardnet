import type { InstallPhase, UpdateChannel, UpdateStatus } from "@wardnet/js";
import { DownloadIcon, Loader2Icon, RefreshCwIcon } from "lucide-react";
import { Button } from "@wardnet/forge/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Switch } from "@/components/core/ui/switch";
import { Label } from "@/components/core/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { StatusBadge } from "@/components/compound/StatusBadge";

/**
 * Whether a given install phase represents work in progress — i.e. a check/
 * download/verify/stage/swap is running server-side and the user should not
 * be able to kick off a second install on top of it.
 */
function isPhaseActive(phase: InstallPhase): boolean {
  switch (phase.phase) {
    case "checking":
    case "downloading":
    case "verifying":
    case "staging":
    case "swapping":
    case "restart_pending":
      return true;
    default:
      return false;
  }
}

function describePhase(phase: InstallPhase): string {
  switch (phase.phase) {
    case "checking":
      return "Checking";
    case "downloading":
      return phase.total
        ? `Downloading (${phase.bytes.toLocaleString()} / ${phase.total.toLocaleString()} bytes)`
        : "Downloading";
    case "verifying":
      return "Verifying";
    case "staging":
      return "Staging";
    case "swapping":
      return "Swapping";
    case "restart_pending":
      return "Restart pending";
    // Failed phase renders its reason in the alert banner; the phase cell
    // only needs to flag the state, not restate the error.
    case "failed":
      return "Failed";
    case "idle":
      return "Idle";
    case "applied":
      return "Applied";
  }
}

interface Props {
  status: UpdateStatus | null;
  isLoading: boolean;
  isChecking: boolean;
  isInstalling: boolean;
  isRollingBack: boolean;
  onCheck: () => void;
  onInstall: () => void;
  onRollback: () => void;
  onToggleAutoUpdate: (enabled: boolean) => void;
  onChangeChannel: (channel: UpdateChannel) => void;
}

/**
 * Pure-presentation card showing version / channel / install state and
 * the admin action buttons. All data + callbacks flow from props; the card
 * itself does no API calls (that is the Settings page's job via hooks).
 */
export function UpdateCard({
  status,
  isLoading,
  isChecking,
  isInstalling,
  isRollingBack,
  onCheck,
  onInstall,
  onRollback,
  onToggleAutoUpdate,
  onChangeChannel,
}: Props) {
  // The install button must consider *both* the in-flight mutation and the
  // server-reported phase — without the latter, clicking Install would kick
  // off a second mutation after the first returns while the daemon is still
  // downloading in the background.
  const phaseActive = status ? isPhaseActive(status.install_phase) : false;
  const installInProgress = isInstalling || phaseActive;

  // The card head's right slot is a state-as-button transformation
  // (WEBUI-DESIGN-GUIDELINES §4.4): a non-interactive "Up to date" badge
  // when there's nothing to do, a primary "Install update" button when
  // an update is available, and a disabled in-progress button while the
  // install is running. The action row below carries only Check + Rollback.
  const renderStateSlot = () => {
    if (!status) return null;
    if (installInProgress) {
      return (
        <Button disabled>
          <Loader2Icon className="animate-spin" />
          {describePhase(status.install_phase)}…
        </Button>
      );
    }
    if (status.update_available) {
      return (
        <Button onClick={onInstall}>
          <DownloadIcon />
          Install update
        </Button>
      );
    }
    return (
      <StatusBadge tone="success" withIcon>
        Up to date
      </StatusBadge>
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Auto-update</CardTitle>
        <CardAction>{renderStateSlot()}</CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {isLoading || !status ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : (
          <>
            <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
              <div>
                <dt className="text-muted-foreground">Current version</dt>
                <dd className="font-medium">{status.current_version}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground">Latest</dt>
                <dd className="font-medium">{status.latest_version ?? "unknown"}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground">Last checked</dt>
                <dd className="font-medium">
                  {status.last_check_at ? new Date(status.last_check_at).toLocaleString() : "never"}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground">Current phase</dt>
                <dd className="font-medium">{describePhase(status.install_phase)}</dd>
              </div>
              {status.pending_version && (
                <div>
                  <dt className="text-muted-foreground">Pending</dt>
                  <dd className="font-medium">{status.pending_version}</dd>
                </div>
              )}
            </dl>

            {status.install_phase.phase === "failed" && (
              <div
                role="alert"
                className="rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive"
              >
                <div className="font-medium">Last install failed</div>
                <div className="mt-0.5 break-words">{status.install_phase.reason}</div>
              </div>
            )}

            {/* Channel + auto-install share one row: the channel is the
                admin's primary choice, the toggle controls whether updates
                land automatically once a new release matches it. */}
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div className="flex items-center gap-3">
                <Label htmlFor="update-channel">Channel</Label>
                <Select
                  value={status.channel}
                  onValueChange={(v) => onChangeChannel(v as UpdateChannel)}
                  disabled={phaseActive}
                >
                  <SelectTrigger id="update-channel" className="w-[160px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="stable">Stable</SelectItem>
                    <SelectItem value="beta">Beta</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="flex items-center gap-3">
                <Switch
                  id="auto-update-toggle"
                  checked={status.auto_update_enabled}
                  onCheckedChange={onToggleAutoUpdate}
                />
                <Label htmlFor="auto-update-toggle">Automatically install when available</Label>
              </div>
            </div>

            {/* Footer action row: helper context on the left, utility
                actions on the right (per WEBUI-DESIGN-GUIDELINES §3.6). */}
            <div className="flex items-center justify-between gap-2 border-t pt-4">
              <p className="text-sm text-muted-foreground">Updates are checked hourly.</p>
              <div className="flex flex-wrap justify-end gap-2">
                {status.rollback_available && (
                  <Button
                    variant="destructive"
                    onClick={onRollback}
                    disabled={isRollingBack || phaseActive}
                  >
                    {isRollingBack ? "Rolling back…" : "Rollback to previous"}
                  </Button>
                )}
                <Button variant="outline" onClick={onCheck} disabled={isChecking || phaseActive}>
                  <RefreshCwIcon className={isChecking ? "animate-spin" : undefined} />
                  {isChecking ? "Checking…" : "Check now"}
                </Button>
              </div>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
