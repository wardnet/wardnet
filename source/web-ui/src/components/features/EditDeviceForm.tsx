import { useState } from "react";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import { Button } from "@/components/core/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { RoutingSelector } from "@/components/compound/RoutingSelector";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useUpdateDevice } from "@/hooks/useDevices";
import { useTunnels } from "@/hooks/useTunnels";
import type { Device, DeviceType, RoutingTarget } from "@wardnet/js";

const DEVICE_TYPE_OPTIONS: { value: DeviceType; label: string }[] = [
  { value: "tv", label: "TV" },
  { value: "phone", label: "Phone" },
  { value: "laptop", label: "Laptop" },
  { value: "tablet", label: "Tablet" },
  { value: "game_console", label: "Console" },
  { value: "settop_box", label: "Set-top Box" },
  { value: "iot", label: "IoT" },
  { value: "router", label: "Router" },
  { value: "managed_switch", label: "Managed Switch" },
  { value: "server", label: "Server" },
  { value: "unknown", label: "Unknown" },
];

interface EditDeviceFormProps {
  device: Device;
  currentRule: RoutingTarget | null;
  onClose: () => void;
  onReserveAddress?: (mac: string, ip: string, hostname?: string) => void;
}

/** Form body for editing a device's name, type, routing, and lock status. */
export function EditDeviceForm({
  device,
  currentRule,
  onClose,
  onReserveAddress,
}: EditDeviceFormProps) {
  const { data: tunnelData } = useTunnels();
  const updateDevice = useUpdateDevice();
  const tunnels = tunnelData?.tunnels ?? [];

  const [name, setName] = useState(device.name ?? "");
  const [deviceType, setDeviceType] = useState<DeviceType>(device.device_type);
  const [routingTarget, setRoutingTarget] = useState<RoutingTarget | null>(currentRule);
  const [adminLocked, setAdminLocked] = useState(device.admin_locked);

  async function handleSave() {
    await updateDevice.mutateAsync({
      id: device.id,
      body: {
        name: name || undefined,
        device_type: deviceType,
        routing_target: routingTarget ?? undefined,
        admin_locked: adminLocked,
      },
    });
    onClose();
  }

  return (
    <div className="mt-6 flex flex-col gap-5">
      <div className="flex items-center gap-3">
        <DeviceIcon type={device.device_type} size={24} className="text-foreground/60" />
        <div>
          <p className="font-medium">{device.name ?? device.hostname ?? device.mac}</p>
          <p className="text-xs text-muted-foreground">{device.mac}</p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="edit-name">Friendly Name</Label>
        <Input
          id="edit-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={device.hostname ?? device.mac}
        />
      </div>

      <div className="flex flex-col gap-2">
        <Label>Device Type</Label>
        <Select value={deviceType} onValueChange={(v) => setDeviceType(v as DeviceType)}>
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {DEVICE_TYPE_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="flex flex-col gap-2">
        <Label>Routing</Label>
        <RoutingSelector
          value={routingTarget}
          onChange={setRoutingTarget}
          tunnels={tunnels}
          isAdmin
        />
      </div>

      <div className="flex items-center justify-between">
        <Label htmlFor="edit-lock">Lock routing (prevent user changes)</Label>
        <Switch id="edit-lock" checked={adminLocked} onCheckedChange={setAdminLocked} />
      </div>

      {onReserveAddress && (
        <Button
          variant="outline"
          className="w-full"
          onClick={() =>
            onReserveAddress(
              device.mac,
              device.last_ip,
              device.name ?? device.hostname ?? undefined,
            )
          }
        >
          Reserve DHCP Address
        </Button>
      )}

      {updateDevice.isError && (
        <ApiErrorAlert error={updateDevice.error} fallback="Failed to update device" />
      )}

      <Button onClick={handleSave} disabled={updateDevice.isPending} className="w-full">
        {updateDevice.isPending ? "Saving..." : "Save Changes"}
      </Button>
    </div>
  );
}
