import { useState } from "react";
import { Card, CardContent } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { RoutingSelector } from "@/components/compound/RoutingSelector";
import { useDevices, useDevice, useUpdateDevice } from "@/hooks/useDevices";
import { useTunnels } from "@/hooks/useTunnels";
import { timeAgo } from "@/lib/utils";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import type { Device, DeviceType, RoutingTarget } from "@wardnet/js";

const DEVICE_TYPE_OPTIONS: { value: DeviceType; label: string }[] = [
  { value: "tv", label: "TV" },
  { value: "phone", label: "Phone" },
  { value: "laptop", label: "Laptop" },
  { value: "tablet", label: "Tablet" },
  { value: "game_console", label: "Console" },
  { value: "settop_box", label: "Set-top Box" },
  { value: "iot", label: "IoT" },
  { value: "unknown", label: "Unknown" },
];

function deviceTypeLabel(type: Device["device_type"]): string {
  return DEVICE_TYPE_OPTIONS.find((o) => o.value === type)?.label ?? type;
}

function EditDeviceSheet({
  deviceId,
  open,
  onOpenChange,
}: {
  deviceId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { data } = useDevice(deviceId);
  const device = data?.device;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Edit Device</SheetTitle>
        {device ? (
          <EditDeviceForm
            key={device.id + String(data?.current_rule?.type)}
            device={device}
            currentRule={data?.current_rule ?? null}
            onClose={() => onOpenChange(false)}
          />
        ) : (
          <p className="mt-6 text-sm text-muted-foreground">Loading device...</p>
        )}
      </SheetContent>
    </Sheet>
  );
}

function EditDeviceForm({
  device,
  currentRule,
  onClose,
}: {
  device: Device;
  currentRule: RoutingTarget | null;
  onClose: () => void;
}) {
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

      {updateDevice.isError && (
        <ApiErrorAlert error={updateDevice.error} fallback="Failed to update device" />
      )}

      <Button onClick={handleSave} disabled={updateDevice.isPending} className="w-full">
        {updateDevice.isPending ? "Saving..." : "Save Changes"}
      </Button>
    </div>
  );
}

/** Devices page showing all discovered network devices. */
export default function Devices() {
  const { data, isLoading, isError } = useDevices();
  const devices = data?.devices ?? [];
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);

  return (
    <>
      <PageHeader title="Devices" />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading devices...
          </CardContent>
        </Card>
      )}

      {isError && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Failed to load devices. Make sure the daemon is running.
          </CardContent>
        </Card>
      )}

      {!isLoading && !isError && devices.length === 0 && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            No devices discovered yet. Devices will appear as they are detected on the network.
          </CardContent>
        </Card>
      )}

      {devices.length > 0 && (
        <div className="overflow-hidden rounded-xl border border-border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border bg-muted/50">
                <th className="px-4 py-3 text-left font-medium text-muted-foreground">Device</th>
                <th className="hidden px-4 py-3 text-left font-medium text-muted-foreground md:table-cell">
                  IP
                </th>
                <th className="hidden px-4 py-3 text-left font-medium text-muted-foreground sm:table-cell">
                  Type
                </th>
                <th className="hidden px-4 py-3 text-left font-medium text-muted-foreground lg:table-cell">
                  Manufacturer
                </th>
                <th className="px-4 py-3 text-right font-medium text-muted-foreground">
                  Last Seen
                </th>
              </tr>
            </thead>
            <tbody>
              {devices.map((device) => (
                <tr
                  key={device.id}
                  className="cursor-pointer border-b border-border last:border-0 hover:bg-muted/50"
                  onClick={() => setSelectedDeviceId(device.id)}
                >
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <DeviceIcon type={device.device_type} />
                      <div className="flex flex-col">
                        <span className="font-medium">
                          {device.name ?? device.hostname ?? device.mac}
                        </span>
                        {(device.name || device.hostname) && (
                          <span className="text-xs text-muted-foreground">{device.mac}</span>
                        )}
                      </div>
                    </div>
                  </td>
                  <td className="hidden px-4 py-3 text-muted-foreground md:table-cell">
                    {device.last_ip}
                  </td>
                  <td className="hidden px-4 py-3 sm:table-cell">
                    <Badge variant="secondary">{deviceTypeLabel(device.device_type)}</Badge>
                  </td>
                  <td className="hidden px-4 py-3 text-muted-foreground lg:table-cell">
                    {device.manufacturer ?? "—"}
                  </td>
                  <td className="px-4 py-3 text-right text-muted-foreground">
                    {timeAgo(device.last_seen)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {selectedDeviceId && (
        <EditDeviceSheet
          deviceId={selectedDeviceId}
          open={!!selectedDeviceId}
          onOpenChange={(open) => {
            if (!open) setSelectedDeviceId(null);
          }}
        />
      )}
    </>
  );
}
