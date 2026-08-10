import { useState } from "react";
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
import { Input } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import { DeviceIcon } from "@wardnet/web";
import { RoutingSelector } from "@wardnet/web";
import { countryFlag } from "@wardnet/web";
import { DEVICE_TYPE_OPTIONS, deviceTypeLabel } from "@wardnet/web";
import type { MutationHandle } from "@/lib/mutationHandle";
import type {
  Device,
  DeviceType,
  RoutingTarget,
  Tunnel,
  UpdateDeviceRequest,
} from "@wardnet/js";

function routingLabel(
  target: RoutingTarget | null,
  tunnels: Tunnel[],
): React.ReactNode {
  // `default` is no longer offered in the picker; treat it as the resolved
  // direct case for display purposes (matches stock daemon behaviour where
  // `network.default_policy = "direct"`).
  if (!target || target.type === "default" || target.type === "direct") {
    return "Direct (no VPN)";
  }
  const tunnel = tunnels.find((t) => t.id === target.tunnel_id);
  if (!tunnel) return "Via tunnel";
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";
  return (
    <>
      Via tunnel: {flag ? <span aria-hidden>{flag} </span> : null}
      {tunnel.label}
    </>
  );
}

interface DeviceSettingsCardProps {
  device: Device;
  currentRule: RoutingTarget | null;
  tunnels: Tunnel[];
  /** The page's hoisted device-update mutation. */
  updateDevice: MutationHandle<{ id: string; body: UpdateDeviceRequest }>;
}

/** Editable settings card: friendly name, type, routing, admin lock. Pure
 *  presentation — the owning page wires the query/mutation hooks and passes
 *  data + callbacks in. */
export function DeviceSettingsCard({
  device,
  currentRule,
  tunnels,
  updateDevice,
}: DeviceSettingsCardProps) {
  // Server-owned flag (#1181), not inferred from the name.
  const isManaged = device.managed;

  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(device.name ?? "");
  const [deviceType, setDeviceType] = useState<DeviceType>(device.device_type);
  const [routingTarget, setRoutingTarget] = useState<RoutingTarget | null>(
    currentRule,
  );
  const [adminLocked, setAdminLocked] = useState(device.admin_locked);

  function startEdit() {
    setName(device.name ?? "");
    setDeviceType(device.device_type);
    setRoutingTarget(currentRule);
    setAdminLocked(device.admin_locked);
    updateDevice.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    updateDevice.reset();
  }

  const trimmedName = name.trim();

  async function handleSave() {
    await updateDevice.mutateAsync({
      id: device.id,
      body: {
        // A managed device is defined by having a name. Sending an empty name
        // leaves the device unmanaged while still persisting routing/type/lock —
        // so editing an unnamed device's routing works without forcing a name.
        name: trimmedName || undefined,
        device_type: deviceType,
        routing_target: routingTarget ?? undefined,
        admin_locked: adminLocked,
      },
    });
    setEditing(false);
  }

  // Always just "Save". Since #1181, saving ANY admin setting here promotes
  // the device to managed — not only a name — so a label that singled out
  // naming would be describing the wrong thing on most of these saves.

  return (
    <Card>
      <CardHeader>
        <CardTitle>Settings</CardTitle>
        {!editing && (
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={startEdit}
              data-testid="device-settings-edit"
            >
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 md:grid-cols-2">
            <Field
              label="Friendly name"
              htmlFor="device-name"
              help={
                isManaged
                  ? undefined
                  : "Optional. Saving any setting here will manage this device."
              }
            >
              <Input
                id="device-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={device.hostname ?? device.mac}
              />
            </Field>

            <Field label="Device type">
              <Select
                value={deviceType}
                onValueChange={(v) => setDeviceType(v as DeviceType)}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {DEVICE_TYPE_OPTIONS.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      <span className="inline-flex items-center gap-2">
                        <DeviceIcon type={opt.value} size={16} />
                        {opt.label}
                      </span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>

            <Field label="Default route">
              <RoutingSelector
                value={routingTarget}
                onChange={setRoutingTarget}
                tunnels={tunnels}
                isAdmin
                data-testid="device-settings-routing"
              />
            </Field>

            <Field label="Admin lock" htmlFor="device-lock">
              <div className="flex h-9 items-center justify-between">
                <Text size="sm" className="text-ink-3">
                  Prevent user routing changes
                </Text>
                <Toggle
                  id="device-lock"
                  checked={adminLocked}
                  onCheckedChange={setAdminLocked}
                />
              </div>
            </Field>

            {updateDevice.isError && (
              <div className="md:col-span-2">
                <ApiErrorAlert
                  error={updateDevice.error}
                  fallback="Failed to update device"
                />
              </div>
            )}
          </CardContent>
          <FormActions
            secondaryLabel="Cancel"
            secondaryProps={{
              onClick: cancelEdit,
              disabled: updateDevice.isPending,
            }}
            primaryLabel={updateDevice.isPending ? "Saving…" : "Save"}
            primaryProps={{
              onClick: handleSave,
              disabled: updateDevice.isPending,
              "data-testid": "device-settings-save",
            }}
          />
        </>
      ) : (
        <CardContent className="grid grid-cols-1 gap-x-6 gap-y-4 md:grid-cols-2">
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              Friendly name
            </Text>
            <Text size="sm">{device.name ?? "-"}</Text>
          </div>
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              Type
            </Text>
            <Text size="sm" className="inline-flex items-center gap-2">
              <DeviceIcon type={device.device_type} size={16} />
              {deviceTypeLabel(device.device_type)}
            </Text>
          </div>
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              Default route
            </Text>
            <Text size="sm" data-testid="device-settings-routing-value">
              {routingLabel(currentRule, tunnels)}
            </Text>
          </div>
          <div className="flex flex-col gap-0.5">
            <Text size="xs" className="uppercase tracking-wide text-ink-3">
              Admin lock
            </Text>
            <Text size="sm">{device.admin_locked ? "Locked" : "Unlocked"}</Text>
          </div>
        </CardContent>
      )}
    </Card>
  );
}
