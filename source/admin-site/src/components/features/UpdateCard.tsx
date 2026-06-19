import type { InstallPhase, UpdateChannel, UpdateStatus } from "@wardnet/js";
import { DownloadIcon, Loader2Icon, RefreshCwIcon } from "lucide-react";
import { Button } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { formatDateTime } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";

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
 * the admin action buttons. Follows the DhcpStatusCard / DnsServer
 * chrome: title + pill + Toggle in CardAction. The release-channel
 * select and the "Check now" button sit in the same CardAction row
 * because they're peers of the toggle (all three are top-level
 * controls for how/when updates are pulled).
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
  const phaseActive = status ? isPhaseActive(status.install_phase) : false;
  const installInProgress = isInstalling || phaseActive;
  const autoEnabled = status?.auto_update_enabled ?? false;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Auto-update</CardTitle>
        <Pill variant={autoEnabled ? "ok" : "ghost"}>
          <span className="dot" />
          {autoEnabled ? "Enabled" : "Disabled"}
        </Pill>
        {/* Header layout:
            - Below 890px (default): Toggle pinned right on row 1
              next to title+pill (`ml-auto`); the Channel + Check
              group wraps to its own full-width row 2 below (via
              `w-full`) with channel-left / check-right.
            - At ≥890px: classes get reset — Toggle drops `ml-auto`,
              the group reclaims content width and uses `order-2`
              to sit between the pill and the toggle. CardAction's
              `margin-left: auto` (from `.right`) then pushes the
              group + toggle to the right edge as a single cluster.
              The breakpoint is `min-[890px]:` rather than `md:`
              because the controls don't actually fit on a single
              row until ~890px; widening Tailwind's global `md`
              token would ripple across the whole app. */}
        <Toggle
          id="auto-update-toggle"
          aria-label="Enable automatic updates"
          className="ml-auto min-[890px]:order-last min-[890px]:ml-0"
          checked={autoEnabled}
          disabled={!status || phaseActive}
          onCheckedChange={onToggleAutoUpdate}
        />
        <CardAction className="w-full justify-between min-[890px]:order-2 min-[890px]:w-auto min-[890px]:justify-end">
          <Select
            value={status?.channel}
            onValueChange={(v) => onChangeChannel(v as UpdateChannel)}
            disabled={!status || phaseActive}
          >
            <SelectTrigger className="select-trigger--md w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="stable">Stable channel</SelectItem>
              <SelectItem value="beta">Beta channel</SelectItem>
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            onClick={onCheck}
            disabled={isChecking || phaseActive}
            aria-label="Check for updates"
          >
            <RefreshCwIcon
              className={isChecking ? "animate-spin" : undefined}
            />
            {isChecking ? "Checking…" : "Check now"}
          </Button>
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {isLoading || !status ? (
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        ) : (
          <>
            <Text as="p" size="sm" className="text-ink-3">
              When enabled, Wardnet checks for updates hourly and installs them
              automatically as they land on the selected channel.
            </Text>

            <Text
              as="dl"
              size="sm"
              className="grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-3"
            >
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Current version
                </Text>
                <Text as="dd" weight="medium">
                  {status.current_version}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Latest
                </Text>
                <Text as="dd" weight="medium">
                  {status.latest_version ?? "unknown"}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Last checked
                </Text>
                <Text as="dd" weight="medium">
                  {status.last_check_at
                    ? formatDateTime(status.last_check_at)
                    : "never"}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Current phase
                </Text>
                <Text as="dd" weight="medium">
                  {describePhase(status.install_phase)}
                </Text>
              </div>
              {status.pending_version && (
                <div>
                  <Text
                    as="dt"
                    size="xs"
                    className="uppercase tracking-wide text-ink-3"
                  >
                    Pending
                  </Text>
                  <Text as="dd" weight="medium">
                    {status.pending_version}
                  </Text>
                </div>
              )}
            </Text>

            {status.install_phase.phase === "failed" && (
              <div
                role="alert"
                className="rounded-md border border-danger/50 bg-danger/10 p-3 text-sm text-danger"
              >
                <Text as="div" weight="medium">
                  Last install failed
                </Text>
                <div className="mt-0.5 break-words">
                  {status.install_phase.reason}
                </div>
              </div>
            )}

            {(status.update_available ||
              installInProgress ||
              status.rollback_available) && (
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
                {installInProgress ? (
                  <Button disabled>
                    <Loader2Icon className="animate-spin" />
                    {describePhase(status.install_phase)}…
                  </Button>
                ) : status.update_available ? (
                  <Button onClick={onInstall}>
                    <DownloadIcon />
                    Install update
                  </Button>
                ) : null}
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
