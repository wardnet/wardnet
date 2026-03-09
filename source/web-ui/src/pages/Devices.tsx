import { Card, CardContent } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { PageHeader } from "@/components/compound/PageHeader";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { useDevices } from "@/hooks/useDevices";
import { timeAgo } from "@/lib/utils";
import type { Device } from "@wardnet/js";

function deviceTypeLabel(type: Device["device_type"]): string {
  const labels: Record<Device["device_type"], string> = {
    tv: "TV",
    phone: "Phone",
    laptop: "Laptop",
    tablet: "Tablet",
    game_console: "Console",
    settop_box: "Set-top Box",
    iot: "IoT",
    unknown: "Unknown",
  };
  return labels[type] ?? type;
}

/** Devices page showing all discovered network devices. */
export default function Devices() {
  const { data, isLoading, isError } = useDevices();
  const devices = data?.devices ?? [];

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
                <tr key={device.id} className="border-b border-border last:border-0">
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
    </>
  );
}
