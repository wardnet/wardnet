import { useState } from "react";
import { Button } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import {
  usePendingDevices,
  useUpdateNetworkZone,
  useAssignDeviceZone,
  useQuarantineNewDevices,
  useSetQuarantineNewDevices,
  deviceDisplayName,
  timeAgo,
} from "@wardnet/web";
import type { Device, NetworkZoneView } from "@wardnet/js";

/**
 * New-device quarantine settings (issue #738). Quarantine is **notification
 * only**: new devices already land in the default-for-new zone unconditionally;
 * the toggle just controls whether admins get a push when one appears. The
 * "pending approvals" list is derived — devices currently sitting in the
 * default-for-new zone, most-recent-first — and "Approve" simply reassigns the
 * device to a real zone.
 */
export function QuarantineSettingsCard() {
  const { data: quarantine } = useQuarantineNewDevices();
  const setQuarantine = useSetQuarantineNewDevices();
  const setDefaultForNew = useUpdateNetworkZone({
    successMessage: "Default-for-new zone updated",
  });
  const { pending, defaultForNew, homeZone, zones } = usePendingDevices();

  return (
    <Card>
      <CardHeader>
        <CardTitle>New-device quarantine</CardTitle>
      </CardHeader>

      <CardContent className="flex flex-col gap-5">
        <label className="flex items-center justify-between gap-4">
          <span className="flex flex-col">
            <Text size="sm">Notify admins about new devices</Text>
            <Text size="xs" className="text-ink-3">
              New devices always join the default-for-new zone. This only
              controls whether admins get a push notification to review them.
            </Text>
          </span>
          <Toggle
            aria-label="Notify admins about new devices"
            data-testid="quarantine-toggle"
            checked={quarantine?.enabled ?? false}
            onCheckedChange={(enabled) => setQuarantine.mutate(enabled)}
            disabled={setQuarantine.isPending}
          />
        </label>

        <Field
          label="Default zone for new devices"
          htmlFor="default-for-new"
          help="Freshly-discovered devices are placed here until an admin moves them."
        >
          <Select
            value={defaultForNew?.id ?? ""}
            onValueChange={(id) =>
              setDefaultForNew.mutate({
                id,
                body: { is_default_for_new: true },
              })
            }
          >
            <SelectTrigger
              id="default-for-new"
              data-testid="quarantine-default-for-new"
              className="w-full sm:w-72"
            >
              <SelectValue placeholder="Select a zone" />
            </SelectTrigger>
            <SelectContent>
              {zones.map((z) => (
                <SelectItem key={z.id} value={z.id}>
                  {z.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <div className="flex flex-col gap-2">
          <Text size="xs" className="uppercase tracking-wide text-ink-3">
            Awaiting review ({pending.length})
          </Text>
          {pending.length === 0 ? (
            <Text size="sm" className="text-ink-3">
              No new devices awaiting review.
            </Text>
          ) : (
            <div className="flex flex-col divide-y divide-line">
              {pending.map((device) => (
                <PendingDeviceRow
                  key={device.id}
                  device={device}
                  zones={zones}
                  defaultTargetZoneId={homeZone?.id}
                />
              ))}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

interface PendingDeviceRowProps {
  device: Device;
  zones: NetworkZoneView[];
  defaultTargetZoneId: string | undefined;
}

function PendingDeviceRow({
  device,
  zones,
  defaultTargetZoneId,
}: PendingDeviceRowProps) {
  const assignZone = useAssignDeviceZone({ successMessage: "Device approved" });
  // Default the approve target to the home zone; fall back to the first zone.
  const [targetZoneId, setTargetZoneId] = useState(
    defaultTargetZoneId ?? zones[0]?.id ?? "",
  );

  return (
    <div
      className="flex flex-wrap items-center justify-between gap-3 py-3"
      data-testid="quarantine-pending-row"
    >
      <div className="flex flex-col">
        <Text size="sm" weight="medium">
          {deviceDisplayName(device)}
        </Text>
        <Text size="xs" className="text-ink-3">
          Joined {timeAgo(device.first_seen)}
        </Text>
      </div>
      <div className="flex items-center gap-2">
        <Select value={targetZoneId} onValueChange={setTargetZoneId}>
          <SelectTrigger className="w-40" aria-label="Approve to zone">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {zones.map((z) => (
              <SelectItem key={z.id} value={z.id}>
                {z.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          size="sm"
          data-testid="quarantine-approve"
          disabled={assignZone.isPending || !targetZoneId}
          onClick={() =>
            assignZone.mutate({ deviceId: device.id, zoneId: targetZoneId })
          }
        >
          Approve
        </Button>
      </div>
    </div>
  );
}
