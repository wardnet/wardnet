import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
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
        <Text
          as="dl"
          size="sm"
          className="grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-3"
        >
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              MAC
            </Text>
            <dd className="font-mono">{device.mac}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Hostname
            </Text>
            <dd>{device.hostname ?? "—"}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Manufacturer
            </Text>
            <dd>{device.manufacturer ?? "—"}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              First seen
            </Text>
            <dd>{timeAgo(device.first_seen)}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Last seen
            </Text>
            <dd>{timeAgo(device.last_seen)}</dd>
          </div>
        </Text>
      </CardContent>
    </Card>
  );
}
