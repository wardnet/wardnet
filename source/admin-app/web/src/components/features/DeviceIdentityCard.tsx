import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { timeAgo } from "@/lib/utils";
import type { Device } from "@wardnet/js";

interface DeviceIdentityCardProps {
  device: Device;
}

/** Read-only identity card: MAC, hostname, manufacturer, first/last seen. */
export function DeviceIdentityCard({ device }: DeviceIdentityCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Identity</CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-2 gap-x-6 md:grid-cols-3">
        <Field label="MAC" editing={false} value={<span className="mono">{device.mac}</span>} />
        <Field label="Hostname" editing={false} value={device.hostname ?? "—"} />
        <Field label="Manufacturer" editing={false} value={device.manufacturer ?? "—"} />
        <Field label="First seen" editing={false} value={timeAgo(device.first_seen)} />
        <Field label="Last seen" editing={false} value={timeAgo(device.last_seen)} />
      </CardContent>
    </Card>
  );
}
