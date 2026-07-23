import { useState } from "react";
import { Button } from "@wardnet/web";
import { Form, FormActions, Validator } from "@wardnet/web";
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
import { useUpdateDevice } from "@wardnet/web";
import { useTunnels } from "@wardnet/web";
import { countryFlag } from "@wardnet/web";
import { DEVICE_TYPE_OPTIONS, deviceTypeLabel } from "@wardnet/web";
import type { Device, DeviceType, RoutingTarget, Tunnel } from "@wardnet/js";

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
}

/** Editable settings card: friendly name, type, routing, admin lock. */
export function DeviceSettingsCard({
  device,
  currentRule,
}: DeviceSettingsCardProps) {
  const isManaged = device.name != null;
  const { data: tunnelData } = useTunnels();
  const tunnels = tunnelData?.tunnels ?? [];
  const updateDevice = useUpdateDevice();

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

  // A managed device is defined by having a name, so a name is required both to
  // promote an unmanaged device and to keep a managed one managed. The `<Form>`
  // only fires `onSubmit` once the `required` validator passes, so an empty name
  // can no longer slip through as a silent no-op.
  async function handleSave(values: { name: string }) {
    await updateDevice.mutateAsync({
      id: device.id,
      body: {
        name: values.name.trim(),
        device_type: deviceType,
        routing_target: routingTarget ?? undefined,
        admin_locked: adminLocked,
      },
    });
    setEditing(false);
  }

  const saveLabel = isManaged ? "Save" : "To Managed Device";
  const savingLabel = isManaged ? "Saving…" : "Promoting…";

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
        <Form values={{ name }} onSubmit={handleSave}>
          <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 md:grid-cols-2">
            <Field label="Friendly name" htmlFor="device-name" name="name">
              <Input
                id="device-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={device.hostname ?? device.mac}
              />
            </Field>
            <Validator
              name="name"
              rule="required"
              message="A name is required to manage this device."
            />

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

            <Field label="Routing">
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
              type: "button",
              onClick: cancelEdit,
              disabled: updateDevice.isPending,
            }}
            primaryLabel={updateDevice.isPending ? savingLabel : saveLabel}
            primaryProps={{
              type: "submit",
              disabled: updateDevice.isPending,
              "data-testid": "device-settings-save",
            }}
          />
        </Form>
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
              Routing
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
