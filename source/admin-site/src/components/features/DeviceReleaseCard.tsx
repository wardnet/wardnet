import { useState } from "react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import type { MutationHandle } from "@/lib/mutationHandle";
import type { Device } from "@wardnet/js";

interface DeviceReleaseCardProps {
  device: Device;
  /** The page's hoisted release mutation. */
  release: MutationHandle<string>;
}

/**
 * "Stop managing" — the explicit release that reverts every admin-set setting
 * on a device and returns it to unmanaged (issue #1181).
 *
 * Rendered only for managed devices: there is nothing to release otherwise.
 *
 * The reverts are enumerated in the card body rather than hidden behind the
 * confirm dialog. Two of them disconnect the device, and one of them
 * (retention) is a delayed deletion — an admin should be able to read the full
 * consequence before clicking anything, not discover it in a modal.
 */
export function DeviceReleaseCard({ device, release }: DeviceReleaseCardProps) {
  const [confirming, setConfirming] = useState(false);

  if (!device.managed) return null;

  async function handleRelease() {
    setConfirming(false);
    await release.mutateAsync(device.id);
  }

  return (
    <Card data-testid="device-release-card">
      <CardHeader>
        <CardTitle>Stop managing</CardTitle>
        <CardAction>
          <Button
            variant="destructive"
            size="sm"
            disabled={release.isPending}
            onClick={() => setConfirming(true)}
            data-testid="device-release-button"
          >
            {release.isPending ? "Releasing…" : "Stop managing"}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="col gap-12">
        <Text as="p" size="sm" className="text-ink-3">
          Reverts every setting below to its default and returns this device to
          Discovered. It keeps working on the network — it just stops being
          configured.
        </Text>
        <ul
          className="col gap-4 pl-16 list-disc"
          data-testid="device-release-effects"
        >
          <li>
            <Text size="sm" weight="medium">
              Revokes Private DNS access — this disconnects the device
            </Text>
          </li>
          <li>
            <Text size="sm" weight="medium">
              Deletes its remote-access credential — this disconnects the device
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Deletes its DHCP reservation, so its IP can change
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Removes any zone exceptions naming it
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Resets its routing rule and routing profiles
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Resets its DNS filter settings and turns off DNS capture
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Clears its name and admin lock
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Returns it to the default zone for new devices
            </Text>
          </li>
          <li>
            <Text size="sm" className="text-ink-3">
              Once released, the device is forgotten after 30 days away
            </Text>
          </li>
        </ul>
      </CardContent>

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Stop managing this device?"
        description={
          "This revokes its Private DNS access and its remote-access credential, " +
          "disconnecting the device, and reverts every other setting to default. " +
          "It will be forgotten after 30 days away. Re-granting later mints new " +
          "credentials — the old ones cannot be restored."
        }
        confirmLabel="Stop managing"
        onConfirm={() => void handleRelease()}
      />
    </Card>
  );
}
