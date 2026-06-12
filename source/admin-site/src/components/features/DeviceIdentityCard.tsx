import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { timeAgo } from "@wardnet/web";
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
      <CardContent>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-4 text-sm md:grid-cols-3">
          <div>
            <dt className="text-xs uppercase tracking-wide text-ink-3">MAC</dt>
            <dd className="font-mono">{device.mac}</dd>
          </div>
          <div>
            <dt className="text-xs uppercase tracking-wide text-ink-3">
              Hostname
            </dt>
            <dd>{device.hostname ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs uppercase tracking-wide text-ink-3">
              Manufacturer
            </dt>
            <dd>{device.manufacturer ?? "—"}</dd>
          </div>
          <div>
            <dt className="text-xs uppercase tracking-wide text-ink-3">
              First seen
            </dt>
            <dd>{timeAgo(device.first_seen)}</dd>
          </div>
          <div>
            <dt className="text-xs uppercase tracking-wide text-ink-3">
              Last seen
            </dt>
            <dd>{timeAgo(device.last_seen)}</dd>
          </div>
        </dl>
      </CardContent>
    </Card>
  );
}
