import { useMemo, useState } from "react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { useNetworkZones, useAssignDeviceZone } from "@wardnet/web";
import { IsolationDisclaimer } from "./IsolationDisclaimer";
import type { Device } from "@wardnet/js";

interface DeviceZoneCardProps {
  device: Device;
}

/**
 * Zone selector card for the device editor. A device belongs to exactly one
 * Network Zone; changing it here re-runs the daemon's per-device egress /
 * admin-UI enforcement. The isolation disclaimer is mandatory wherever
 * isolation is implied (issue #739).
 */
export function DeviceZoneCard({ device }: DeviceZoneCardProps) {
  const { data: zoneData } = useNetworkZones();
  const assignZone = useAssignDeviceZone({ successMessage: "Zone updated" });
  const zones = useMemo(() => zoneData?.zones ?? [], [zoneData]);

  const [editing, setEditing] = useState(false);
  const [zoneId, setZoneId] = useState(device.zone_id);

  const currentZone = zones.find((z) => z.id === device.zone_id);

  function startEdit() {
    setZoneId(device.zone_id);
    assignZone.reset();
    setEditing(true);
  }

  async function handleSave() {
    await assignZone.mutateAsync({ deviceId: device.id, zoneId });
    setEditing(false);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Zone</CardTitle>
        {!editing && (
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={startEdit}
              data-testid="device-zone-edit"
            >
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {editing ? (
          <>
            <Field label="Network zone" htmlFor="device-zone">
              <Select value={zoneId} onValueChange={setZoneId}>
                <SelectTrigger
                  id="device-zone"
                  data-testid="device-zone-select"
                  className="w-full sm:w-72"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {zones.map((z) => (
                    <SelectItem key={z.id} value={z.id}>
                      {z.name}
                      {z.is_default ? " (home)" : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <IsolationDisclaimer />
            <FormActions
              secondaryLabel="Cancel"
              secondaryProps={{
                type: "button",
                onClick: () => setEditing(false),
                disabled: assignZone.isPending,
              }}
              primaryLabel={assignZone.isPending ? "Saving…" : "Save"}
              primaryProps={{
                type: "button",
                onClick: handleSave,
                disabled: assignZone.isPending,
                "data-testid": "device-zone-save",
              }}
            />
          </>
        ) : (
          <>
            <div className="flex flex-col gap-0.5">
              <Text size="xs" className="uppercase tracking-wide text-ink-3">
                Network zone
              </Text>
              <Text size="sm" data-testid="device-zone-value">
                {currentZone ? (
                  <span className="inline-flex items-center gap-2">
                    {currentZone.name}
                    {currentZone.is_default && (
                      <Pill variant="ghost">Home</Pill>
                    )}
                  </span>
                ) : (
                  "-"
                )}
              </Text>
            </div>
            <IsolationDisclaimer />
          </>
        )}
      </CardContent>
    </Card>
  );
}
